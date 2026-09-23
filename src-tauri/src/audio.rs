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
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, info};
use tsclientlib::audio::AudioHandler;
use tsclientlib::sync::SyncConnectionHandle;
use tsclientlib::ClientId;
use tsproto_packets::packets::{AudioData, CodecType, InAudioBuf, OutAudio, OutPacket};

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
    /// 收包队列（分说话人、重排、PLC/FEC、混音）：连接任务喂包，输出回调消费。
    pub handler: Arc<StdMutex<AudioHandler<ClientId>>>,
    _input: Stream,
    _output: Stream,
}

/// 音频侧 → 异步事件任务的桥（回调线程只 send，不阻塞）。
#[derive(Debug)]
pub enum AudioEvent {
    /// 播放端发现说话人队列结束（fill_buffer 返回）。
    TalkerStopped(ClientId),
    /// 设备错误；事件任务销毁流并发 audio://device-error，健康任务重建。
    DeviceError { input: bool, message: String },
}

/// 音频生命周期管理：连接期间至多一条发送流 + 一条播放流。
pub struct AudioManager {
    /// 发送开关（按住说话）。音频回调只读。
    pub transmit: Arc<AtomicBool>,
    /// 麦克风电平（f32 bits 存 RMS，0..1）。回调写，事件任务读（限频推送）。
    pub mic_level: Arc<AtomicU32>,
    /// 播放混音电平。输出回调写。
    pub out_level: Arc<AtomicU32>,
    /// 当前注册的连接 id；0 = 无。
    active_conn: Arc<AtomicU64>,
    /// cpal 流集合；drop 即停止收发。
    streams: Arc<StdMutex<Option<AudioStreams>>>,
    /// 编码包通道（rx 归发送任务）。
    send_tx: Arc<StdMutex<Option<mpsc::Sender<OutPacket>>>>,
    /// 回调线程 → 事件任务。
    event_tx: UnboundedSender<AudioEvent>,
    event_rx: StdMutex<Option<UnboundedReceiver<AudioEvent>>>,
}

// 手写 Clone：event_rx 是一次性接收端，不随克隆复制。
impl Clone for AudioManager {
    fn clone(&self) -> Self {
        Self {
            transmit: self.transmit.clone(),
            mic_level: self.mic_level.clone(),
            out_level: self.out_level.clone(),
            active_conn: self.active_conn.clone(),
            streams: self.streams.clone(),
            send_tx: self.send_tx.clone(),
            event_tx: self.event_tx.clone(),
            event_rx: StdMutex::new(None),
        }
    }
}

impl AudioManager {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            transmit: Arc::new(AtomicBool::new(false)),
            mic_level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            out_level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            active_conn: Arc::new(AtomicU64::new(0)),
            streams: Arc::new(StdMutex::new(None)),
            send_tx: Arc::new(StdMutex::new(None)),
            event_tx,
            event_rx: StdMutex::new(Some(event_rx)),
        }
    }

    pub fn active_conn(&self) -> u64 {
        self.active_conn.load(Ordering::Relaxed)
    }

    /// 事件任务启动时取走接收端（只允许一个消费者）。
    pub fn take_event_rx(&self) -> Option<UnboundedReceiver<AudioEvent>> {
        self.event_rx.lock().unwrap().take()
    }

    /// 连接任务启动时注册发送通道；音频流要等真正 connected（publish_state）才创建。
    /// 重复连接会覆盖旧注册并使旧发送任务自然退出。
    pub fn register_connection(&self, conn_id: u64, send_tx: mpsc::Sender<OutPacket>) {
        *self.send_tx.lock().unwrap() = Some(send_tx);
        self.active_conn.store(conn_id, Ordering::Relaxed);
    }

    /// connected 后创建收发流；幂等。设备失败只记录，由健康任务重试。
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
        let handler = Arc::new(StdMutex::new(AudioHandler::new()));
        let input =
            open_capture(send_tx, self.transmit.clone(), self.mic_level.clone(), self.event_tx.clone());
        let output = open_playback(
            handler.clone(),
            self.out_level.clone(),
            self.event_tx.clone(),
        );
        match (input, output) {
            (Ok(input), Ok(output)) => {
                *streams = Some(AudioStreams { conn_id, handler, _input: input, _output: output });
                info!("音频收发流已创建");
            }
            (Err(e), _) | (Ok(_), Err(e)) => {
                error!("创建音频流失败（等待自动重试）：{e}");
            }
        }
    }

    /// 设备出错后丢弃当前流；健康任务会用系统默认设备重建。
    pub fn invalidate(&self) {
        self.streams.lock().unwrap().take();
    }

    /// 连接结束时销毁全部音频流与注册信息（只允许一条发送流）。
    pub fn stop(&self) {
        self.active_conn.store(0, Ordering::Relaxed);
        self.streams.lock().unwrap().take();
        *self.send_tx.lock().unwrap() = None;
        self.transmit.store(false, Ordering::Relaxed);
        self.mic_level.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.out_level.store(0.0f32.to_bits(), Ordering::Relaxed);
    }

    /// 连接任务喂入收到的音频包；返回因该包新开始的说话人（用于 TalkingState）。
    pub fn play_packet(&self, from: u16, packet: InAudioBuf) -> Option<ClientId> {
        let streams = self.streams.lock().unwrap();
        let mut handler = streams.as_ref()?.handler.lock().unwrap();
        match handler.handle_packet(ClientId(from), packet) {
            Ok(started) => started,
            Err(e) => {
                debug!("忽略音频包：{e}");
                None
            }
        }
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
    event_tx: UnboundedSender<AudioEvent>,
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
            move |e| {
                // 回调线程：只上报，不重建不触碰 Webview。
                let _ = event_tx.send(AudioEvent::DeviceError { input: true, message: e.to_string() });
            },
            Some(std::time::Duration::from_secs(5)),
        )
        .map_err(|e| format!("打开采集设备 {device_name} 失败：{e}"))?;
    stream
        .play()
        .map_err(|e| format!("启动采集流失败：{e}"))?;
    info!(device = %device_name, "采集设备已打开：48kHz 单声道，20ms 帧");
    Ok(stream)
}

/// 打开系统默认播放设备；M0 同款 48kHz 立体声配置，混音交给 AudioHandler。
fn open_playback(
    handler: Arc<StdMutex<AudioHandler<ClientId>>>,
    out_level: Arc<AtomicU32>,
    event_tx: UnboundedSender<AudioEvent>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device: Device = host
        .default_output_device()
        .ok_or_else(|| "没有可用的音频输出设备".to_string())?;
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "未知设备".into());
    let config = StreamConfig {
        channels: 2,
        sample_rate: 48_000,
        buffer_size: cpal::BufferSize::Fixed(USUAL_FRAME_SIZE as u32),
    };

    let stream = device
        .build_output_stream(
            config,
            {
                let event_tx = event_tx.clone();
                move |data: &mut [f32], _| {
                    // 先清零（fill_buffer 不清空 buf），再由分说话人队列混音。
                    for d in data.iter_mut() {
                        *d = 0.0;
                    }
                    let stopped = handler.lock().unwrap().fill_buffer(data);
                    let rms = rms_of(data);
                    out_level.store(rms.to_bits(), Ordering::Relaxed);
                    for id in stopped {
                        let _ = event_tx.send(AudioEvent::TalkerStopped(id));
                    }
                }
            },
            move |e| {
                let _ = event_tx.send(AudioEvent::DeviceError { input: false, message: e.to_string() });
            },
            Some(std::time::Duration::from_secs(5)),
        )
        .map_err(|e| format!("打开播放设备 {device_name} 失败：{e}"))?;
    stream
        .play()
        .map_err(|e| format!("启动播放流失败：{e}"))?;
    info!(device = %device_name, "播放设备已打开：48kHz 立体声，20ms 帧");
    Ok(stream)
}

/// 事件转发任务：设备错误 → 销毁流 + audio://device-error（回退由健康任务用
/// 系统默认设备完成）；说话结束事件在 M2c 接入 TalkingState。
pub async fn event_relay_task(app: AppHandle, state: crate::app_state::AppState) {
    let Some(mut rx) = state.audio.take_event_rx() else {
        return;
    };
    while let Some(event) = rx.recv().await {
        match event {
            AudioEvent::DeviceError { input, message } => {
                error!(input, %message, "音频设备错误，销毁当前流等待重建");
                state.audio.invalidate();
                let direction = if input { "输入" } else { "输出" };
                let _ = app.emit(
                    "audio://device-error",
                    json!({ "message": format!("音频{direction}设备出错（{message}），正在回退到系统默认设备") }),
                );
            }
            AudioEvent::TalkerStopped(_) => {
                // M2c：接入 TalkingState 后广播 voice://talking(false)
            }
        }
    }
}

/// 电平推送任务：100ms 限频读原子量，发 voice://level（不含原始采样）。
pub async fn level_task(app: AppHandle, state: crate::app_state::AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    loop {
        interval.tick().await;
        if state.audio.active_conn() == 0 {
            continue;
        }
        let mic = f32::from_bits(state.audio.mic_level.load(Ordering::Relaxed));
        let out = f32::from_bits(state.audio.out_level.load(Ordering::Relaxed));
        let _ = app.emit("voice://level", json!({ "mic": mic, "out": out }));
    }
}

/// 健康任务：连接期间若流缺失（设备错误/初始失败），每 2s 用系统默认设备重建。
pub async fn health_task(state: crate::app_state::AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        interval.tick().await;
        let conn = state.audio.active_conn();
        if conn != 0 {
            state.audio.ensure_started(conn);
        }
    }
}

#[tauri::command]
pub async fn set_transmit_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.audio.set_transmit(enabled);
    Ok(())
}
