//! M2 音频链路：把 M0 已验收的采集 → Opus → send_audio 算法接入 Tauri 生命周期。
//! 算法与量纲同 M0（tsclientlib audio 示例的 audio_utils）：48kHz 单声道、
//! 20ms 帧、Opus Voip；本模块不重写 Opus/AudioQueue。
//!
//! 线程规则（docs/02 §3 与 M1 教训）：
//! - cpal 回调只读原子量、只 try_send 有界通道，不 await、不触碰 Tauri/Webview；
//! - 发送任务由 Tokio 运行时实际轮询（M0 教训：LocalSet 未驱动=无声）；
//! - 发送经 SyncConnectionHandle.with_connection，绝不放在驱动事件流的任务里
//!   await handle 操作（M1：自死锁根因）。

use crate::app_state::AppState;
use audiopus::coder::Encoder;
use audiopus::{Application, Channels, SampleRate};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::State;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use tsclientlib::sync::SyncConnectionHandle;
use tsproto_packets::packets::{AudioData, CodecType, OutAudio, OutPacket};

/// M0 基线帧：48kHz、20ms（50 帧/秒）、单声道 → 960 采样。
const USUAL_FRAME_SIZE: usize = 48_000 / 50;
/// RFC6716 规定的 Opus 单帧上限。
const MAX_OPUS_FRAME_SIZE: usize = 1275;
/// 编码包队列容量；满则丢弃（音频回调禁止阻塞）。
pub const SEND_QUEUE: usize = 16;

/// 连接内 audio 发送任务：拥有独立 handle，等待编码包并转发给服务器。
/// 通道关闭（断开/流销毁）或连接消失时自然结束。
pub async fn send_task(mut handle: SyncConnectionHandle, mut rx: mpsc::Receiver<OutPacket>) {
    while let Some(packet) = rx.recv().await {
        let res = handle
            .with_connection(move |con| con.send_audio(packet).map_err(|e| e.to_string()))
            .await;
        match res {
            Err(e) => {
                debug!("音频发送终止（连接已不存在）：{e}");
                break;
            }
            Ok(Err(e)) => debug!("send_audio 失败：{e}"),
            Ok(Ok(())) => {}
        }
    }
}

pub struct AudioStreams {
    pub conn_id: u64,
    _input: Stream,
}

/// 音频生命周期管理：连接期间至多一条发送流。
#[derive(Clone)]
pub struct AudioManager {
    /// 发送开关（按住说话）。音频回调只读。
    pub transmit: Arc<AtomicBool>,
    /// 麦克风电平（f32 bits 存 RMS，0..1）。回调写，事件任务读（M2b 推送）。
    pub mic_level: Arc<AtomicU32>,
    /// 当前注册的连接 id；0 = 无。
    active_conn: Arc<AtomicU64>,
    /// cpal 流集合；drop 即停止采集。
    streams: Arc<StdMutex<Option<AudioStreams>>>,
    /// 编码包通道（rx 归发送任务）。
    send_tx: Arc<StdMutex<Option<mpsc::Sender<OutPacket>>>>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            transmit: Arc::new(AtomicBool::new(false)),
            mic_level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            active_conn: Arc::new(AtomicU64::new(0)),
            streams: Arc::new(StdMutex::new(None)),
            send_tx: Arc::new(StdMutex::new(None)),
        }
    }

    /// 连接任务启动时注册发送通道；音频流要等真正 connected（publish_state）才创建。
    /// 重复连接会覆盖旧注册并使旧发送任务自然退出。
    pub fn register_connection(&self, conn_id: u64, send_tx: mpsc::Sender<OutPacket>) {
        *self.send_tx.lock().unwrap() = Some(send_tx);
        self.active_conn.store(conn_id, Ordering::Relaxed);
    }

    /// connected 后创建发送流；幂等。设备失败只记录，由 M2b 的健康任务重试。
    pub fn ensure_started(&self, conn_id: u64) {
        if self.active_conn.load(Ordering::Relaxed) != conn_id {
            return;
        }
        let mut streams = self.streams.lock().unwrap();
        if streams.as_ref().map(|s| s.conn_id) == Some(conn_id) {
            return;
        }
        let Some(send_tx) = self.send_tx.lock().unwrap().clone() else {
            return;
        };
        match open_capture(send_tx, self.transmit.clone(), self.mic_level.clone()) {
            Ok(input) => {
                *streams = Some(AudioStreams { conn_id, _input: input });
                info!("音频发送流已创建");
            }
            Err(e) => error!("创建音频发送流失败：{e}"),
        }
    }

    /// 连接结束时销毁全部音频流与注册信息（只允许一条发送流）。
    pub fn stop(&self) {
        self.active_conn.store(0, Ordering::Relaxed);
        *self.streams.lock().unwrap() = None;
        *self.send_tx.lock().unwrap() = None;
        self.transmit.store(false, Ordering::Relaxed);
    }

    /// 发送开关。关闭时补 2 个空 Opus 包，让服务器/其他客户端尽快结束“说话中”。
    pub fn set_transmit(&self, enabled: bool) {
        let prev = self.transmit.swap(enabled, Ordering::Relaxed);
        if prev != enabled && !enabled {
            if let Some(tx) = self.send_tx.lock().unwrap().clone() {
                for _ in 0..2 {
                    let packet =
                        OutAudio::new(&AudioData::C2S { id: 0, codec: CodecType::OpusVoice, data: &[] });
                    let _ = tx.try_send(packet);
                }
            }
        }
    }
}

fn rms_of(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: f32 = data.iter().map(|s| s * s).sum();
    (sum / data.len() as f32).sqrt().min(1.0)
}

/// 打开系统默认采集设备；M0 同款配置。设备名与实际配置写日志（M3 设备切换准备）。
fn open_capture(
    send_tx: mpsc::Sender<OutPacket>,
    transmit: Arc<AtomicBool>,
    mic_level: Arc<AtomicU32>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device: Device = host
        .default_input_device()
        .ok_or_else(|| "没有可用的音频输入设备".to_string())?;
    // cpal 0.18：设备名经 description() 获取（含厂商/接口等元数据）。
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "未知设备".into());
    let config = StreamConfig {
        channels: 1,
        sample_rate: 48_000,
        buffer_size: cpal::BufferSize::Fixed(USUAL_FRAME_SIZE as u32),
    };
    let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
        .map_err(|e| format!("创建 Opus 编码器失败：{e}"))?;
    let mut opus_output = [0u8; MAX_OPUS_FRAME_SIZE];

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _| {
                // 电平始终计算（M3 VAD/界面复用）；原始采样不出回调。
                let rms = rms_of(data);
                mic_level.store(rms.to_bits(), Ordering::Relaxed);
                if !transmit.load(Ordering::Relaxed) {
                    return;
                }
                match encoder.encode_float(data, &mut opus_output) {
                    Ok(len) => {
                        let packet = OutAudio::new(&AudioData::C2S {
                            id: 0,
                            codec: CodecType::OpusVoice,
                            data: &opus_output[..len],
                        });
                        if let Err(e) = send_tx.try_send(packet) {
                            if matches!(e, tokio::sync::mpsc::error::TrySendError::Full(_)) {
                                debug!("发送队列满，丢弃音频帧");
                            }
                        }
                    }
                    Err(e) => error!("Opus 编码失败：{e}"),
                }
            },
            move |e| error!("音频采集设备错误：{e}"),
            Some(std::time::Duration::from_secs(5)),
        )
        .map_err(|e| format!("打开采集设备 {device_name} 失败：{e}"))?;
    stream
        .play()
        .map_err(|e| format!("启动采集流失败：{e}"))?;
    info!(device = %device_name, "采集设备已打开：48kHz 单声道，20ms 帧");
    Ok(stream)
}

#[tauri::command]
pub async fn set_transmit_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.audio.set_transmit(enabled);
    Ok(())
}
