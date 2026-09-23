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
use nnnoiseless::DenoiseState;
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
/// VAD 挂起时长：停止后 250ms 再补静音尾包（docs/07 B3，禁止加大）。
const VAD_HANGOVER_MS: u64 = 250;
/// nnnoiseless 每帧样本数（48kHz 下即 10ms）。
const DENOISE_FRAME: usize = 480;

/// 采集/播放参数快照（设置命令更新；ensure_started 时读取）。
#[derive(Clone, Debug)]
pub struct VoiceParams {
    /// "ptt" | "vad"
    pub mode: String,
    pub vad_threshold: f32,
    pub denoise: bool,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            mode: "ptt".into(),
            vad_threshold: 0.5,
            denoise: false,
            input_device: None,
            output_device: None,
        }
    }
}

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
    /// 界面按钮按住来源（A3 双来源之一）。
    pub button_ptt: Arc<AtomicBool>,
    /// 全局热键按住来源（钩子监视任务写，A3 双来源之二）。
    pub key_ptt: Arc<AtomicBool>,
    /// 最终发送开关 = button_ptt || key_ptt（VAD 在阶段 B 接入）。
    /// 音频回调只读。
    pub transmit: Arc<AtomicBool>,
    /// VAD 门控（VAD 模式下采集回调维护：概率≥阈值开、挂起后关）。
    pub vad_active: Arc<AtomicBool>,
    /// 最新语音概率（f32 bits；process_frame 返回值，限频推送用）。
    pub vad_prob: Arc<AtomicU32>,
    /// 采集/播放参数（设置命令同步；重建流时读取）。
    voice_params: Arc<StdMutex<VoiceParams>>,
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
            button_ptt: self.button_ptt.clone(),
            key_ptt: self.key_ptt.clone(),
            transmit: self.transmit.clone(),
            vad_active: self.vad_active.clone(),
            vad_prob: self.vad_prob.clone(),
            voice_params: self.voice_params.clone(),
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
            button_ptt: Arc::new(AtomicBool::new(false)),
            key_ptt: Arc::new(AtomicBool::new(false)),
            transmit: Arc::new(AtomicBool::new(false)),
            vad_active: Arc::new(AtomicBool::new(false)),
            vad_prob: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            voice_params: Arc::new(StdMutex::new(VoiceParams::default())),
            mic_level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            out_level: Arc::new(AtomicU32::new(0.0f32.to_bits())),
            active_conn: Arc::new(AtomicU64::new(0)),
            streams: Arc::new(StdMutex::new(None)),
            send_tx: Arc::new(StdMutex::new(None)),
            event_tx,
            event_rx: StdMutex::new(Some(event_rx)),
        }
    }

    /// 设置命令同步参数快照（与 config.json 一起更新）。
    pub fn set_voice_params(&self, params: VoiceParams) {
        *self.voice_params.lock().unwrap() = params;
    }

    pub fn voice_params(&self) -> VoiceParams {
        self.voice_params.lock().unwrap().clone()
    }

    /// 设置热生效：丢弃当前流并用新参数立即重建（不重连服务器）。
    pub fn rebuild(&self) {
        self.streams.lock().unwrap().take();
        let conn = self.active_conn.load(Ordering::Relaxed);
        if conn != 0 {
            self.ensure_started(conn);
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
        let params = self.voice_params();
        let input = open_capture(
            &params,
            send_tx,
            self.transmit.clone(),
            self.vad_active.clone(),
            self.vad_prob.clone(),
            self.mic_level.clone(),
            self.event_tx.clone(),
        );
        let output = open_playback(
            params.output_device.as_deref(),
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
        self.button_ptt.store(false, Ordering::Relaxed);
        self.key_ptt.store(false, Ordering::Relaxed);
        self.transmit.store(false, Ordering::Relaxed);
        self.vad_active.store(false, Ordering::Relaxed);
        self.vad_prob.store(0.0f32.to_bits(), Ordering::Relaxed);
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

    /// 发送开关（A3 双来源语义）：界面按钮与全局热键任一按住即发送，
    /// 两个都松开才停（同时按住、先松一个 → 继续发送）。
    /// 关闭瞬间补 2 个空 Opus 包，让服务器/其他客户端尽快结束“说话中”。
    fn recompute_transmit(&self) {
        let enabled = self.button_ptt.load(Ordering::Relaxed) || self.key_ptt.load(Ordering::Relaxed);
        let prev = self.transmit.swap(enabled, Ordering::Relaxed);
        if prev && !enabled {
            if let Some(tx) = self.send_tx.lock().unwrap().clone() {
                for _ in 0..2 {
                    let packet =
                        OutAudio::new(&AudioData::C2S { id: 0, codec: CodecType::OpusVoice, data: &[] });
                    let _ = tx.try_send(packet);
                }
            }
        }
    }

    pub fn set_button_ptt(&self, enabled: bool) {
        self.button_ptt.store(enabled, Ordering::Relaxed);
        self.recompute_transmit();
    }

    pub fn set_key_ptt(&self, enabled: bool) {
        self.key_ptt.store(enabled, Ordering::Relaxed);
        self.recompute_transmit();
    }
}

fn rms_of(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: f32 = data.iter().map(|s| s * s).sum();
    (sum / data.len() as f32).sqrt().min(1.0)
}

/// 按名称查找设备；None 或找不到时回退系统默认并打日志（docs/07 B4/B5）。
fn find_input_device(name: Option<&str>) -> Result<Device, String> {
    let host = cpal::default_host();
    if let Some(want) = name {
        if let Some(dev) = host
            .input_devices()
            .ok()
            .and_then(|mut ds| ds.find(|d| d.description().map(|x| x.name() == want).unwrap_or(false)))
        {
            return Ok(dev);
        }
        error!(device = %want, "指定的输入设备不可用，回退系统默认");
    }
    host.default_input_device().ok_or_else(|| "没有可用的音频输入设备".to_string())
}

fn find_output_device(name: Option<&str>) -> Result<Device, String> {
    let host = cpal::default_host();
    if let Some(want) = name {
        if let Some(dev) = host
            .output_devices()
            .ok()
            .and_then(|mut ds| ds.find(|d| d.description().map(|x| x.name() == want).unwrap_or(false)))
        {
            return Ok(dev);
        }
        error!(device = %want, "指定的输出设备不可用，回退系统默认");
    }
    host.default_output_device().ok_or_else(|| "没有可用的音频输出设备".to_string())
}

/// 打开采集设备并搭建"降噪 → VAD → 20ms 重分段 → Opus → 有界队列"管线。
///
/// - cpal 回调周期由驱动决定（本机 480 采样/10ms）；nnnoiseless 恰好按 480 样本
///   逐帧处理（16-bit 量纲），因此采集采样先聚满 480 再进降噪，输出再按
///   USUAL_FRAME_SIZE(960) 重分段编码，线上帧长固定 20ms。
/// - 降噪推理始终运行（docs/07 B2）：概率是 VAD 的输出，"降噪关"只旁路送样
///   （把原始帧送去编码），绝不停推理。
/// - 发送判定（B3）：PTT 模式 = PTT 按住；VAD 模式 = 概率≥阈值，停止后挂起
///   250ms 再补静音尾包。挂起期照常出帧。
#[allow(clippy::too_many_arguments)]
fn open_capture(
    params: &VoiceParams,
    send_tx: mpsc::Sender<OutPacket>,
    transmit: Arc<AtomicBool>,
    vad_active: Arc<AtomicBool>,
    vad_prob: Arc<AtomicU32>,
    mic_level: Arc<AtomicU32>,
    event_tx: UnboundedSender<AudioEvent>,
) -> Result<Stream, String> {
    let device = find_input_device(params.input_device.as_deref())?;
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "未知设备".into());
    let config = StreamConfig {
        channels: 1,
        sample_rate: 48_000,
        buffer_size: cpal::BufferSize::Fixed(DENOISE_FRAME as u32),
    };
    let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
        .map_err(|e| format!("创建 Opus 编码器失败：{e}"))?;
    let mut opus_output = [0u8; MAX_OPUS_FRAME_SIZE];



    // 降噪/VAD 状态（回调线程私有，无锁）。
    let mut denoiser = DenoiseState::new();
    // 首帧淡入伪影（docs/07 B1）：降噪输出的第一帧丢弃。
    let mut denoise_warmup = 1usize;
    let mut in_buf = [0f32; DENOISE_FRAME];
    let mut in_fill = 0usize;
    let mut proc_buf = [0f32; DENOISE_FRAME];
    let mut frame_buf = [0f32; USUAL_FRAME_SIZE];
    let mut frame_fill = 0usize;
    // VAD 门控状态机：概率≥阈值 → 开并刷新挂起计数；概率<阈值 → 挂起递减，
    // 归零关门并补静音尾包。挂起期照常出帧（不靠少发帧表达挂起）。
    let mode_is_vad = params.mode == "vad";
    let threshold = params.vad_threshold.clamp(0.1, 0.9);
    let hangover_frames = (VAD_HANGOVER_MS / 10).max(1) as usize;
    let mut vad_gate = false;
    let mut hangover = 0usize;
    let denoise_enabled = params.denoise;

    info!(
        device = %device_name,
        mode = %params.mode,
        threshold = %threshold,
        denoise = params.denoise,
        "采集设备已打开：48kHz 单声道，20ms 帧，降噪推理常开"
    );

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _| {
                // 电平始终计算；原始采样不出回调。
                let rms = rms_of(data);
                mic_level.store(rms.to_bits(), Ordering::Relaxed);
                let ptt_held = transmit.load(Ordering::Relaxed);
                for &s in data {
                    // ---- 聚满 480 样本 → 降噪推理（始终运行，B2） ----
                    in_buf[in_fill] = s;
                    in_fill += 1;
                    if in_fill < DENOISE_FRAME {
                        continue;
                    }
                    in_fill = 0;
                    // -1..1 → 16-bit 量纲（nnnoiseless 契约，B1）。
                    let mut scaled = [0f32; DENOISE_FRAME];
                    for (o, &v) in scaled.iter_mut().zip(in_buf.iter()) {
                        *o = (v * 32_768.0).clamp(-32_768.0, 32_767.0);
                    }
                    let prob = denoiser.process_frame(&mut proc_buf, &scaled);
                    // 默认模型的 vad 输出偶可略超 1（响亮纯音），钳回 0..1 供 UI/阈值用。
                    let prob = prob.clamp(0.0, 1.0);
                    vad_prob.store(prob.to_bits(), Ordering::Relaxed);
                    if denoise_warmup > 0 {
                        // 丢弃首帧淡入伪影。
                        denoise_warmup -= 1;
                        continue;
                    }
                    // ---- VAD 门控（仅 VAD 模式驱动发送；概率恒更新） ----
                    let send;
                    if mode_is_vad {
                        if prob >= threshold {
                            vad_gate = true;
                            hangover = hangover_frames;
                        } else if vad_gate {
                            hangover = hangover.saturating_sub(1);
                            if hangover == 0 {
                                vad_gate = false;
                                // 关门：补静音尾包，让对端尽快结束“说话中”。
                                for _ in 0..2 {
                                    let packet = OutAudio::new(&AudioData::C2S {
                                        id: 0,
                                        codec: CodecType::OpusVoice,
                                        data: &[],
                                    });
                                    let _ = send_tx.try_send(packet);
                                }
                            }
                        }
                        send = vad_gate;
                        vad_active.store(vad_gate, Ordering::Relaxed);
                    } else {
                        send = ptt_held;
                    }
                    // ---- 产出 480 样本（降噪输出或旁路原始帧，归一化回 -1..1） ----
                    let produced = if denoise_enabled { &proc_buf } else { &scaled };
                    for &v in produced.iter() {
                        let v = (v / 32_768.0).clamp(-1.0, 1.0);
                        if !send {
                            continue;
                        }
                        frame_buf[frame_fill] = v;
                        frame_fill += 1;
                        if frame_fill < USUAL_FRAME_SIZE {
                            continue;
                        }
                        frame_fill = 0;
                        match encoder.encode_float(&frame_buf, &mut opus_output) {
                            Ok(len) => {
                                let packet = OutAudio::new(&AudioData::C2S {
                                    id: 0,
                                    codec: CodecType::OpusVoice,
                                    data: &opus_output[..len],
                                });
                                if let Err(e) = send_tx.try_send(packet) {
                                    if matches!(
                                        e,
                                        tokio::sync::mpsc::error::TrySendError::Full(_)
                                    ) {
                                        debug!("发送队列满，丢弃音频帧");
                                    }
                                }
                            }
                            Err(e) => error!("Opus 编码失败：{e}"),
                        }
                    }
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
    Ok(stream)
}

/// 打开播放设备（按设置选择，缺省系统默认）；48kHz 立体声，混音交给 AudioHandler。
fn open_playback(
    device_name: Option<&str>,
    handler: Arc<StdMutex<AudioHandler<ClientId>>>,
    out_level: Arc<AtomicU32>,
    event_tx: UnboundedSender<AudioEvent>,
) -> Result<Stream, String> {
    let device = find_output_device(device_name)?;
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
                // B5 回退：若错误来源是用户指定的设备，清掉选择并持久化，
                // 健康任务此后用系统默认设备重建。
                {
                    let mut config = state.config.lock().await;
                    let changed = if input {
                        config.voice.input_device.take().is_some()
                    } else {
                        config.voice.output_device.take().is_some()
                    };
                    if changed {
                        if let Err(e) = crate::persistence::save_config(&config) {
                            error!("回退默认设备时保存配置失败：{e}");
                        }
                        let params = state.audio.voice_params();
                        let mut p = params;
                        if input {
                            p.input_device = None;
                        } else {
                            p.output_device = None;
                        }
                        state.audio.set_voice_params(p);
                    }
                }
                let direction = if input { "输入" } else { "输出" };
                let _ = app.emit(
                    "audio://device-error",
                    json!({ "message": format!("音频{direction}设备出错（{message}），已回退到系统默认设备") }),
                );
            }
            AudioEvent::TalkerStopped(id) => {
                // C2：播放队列耗尽 → 该说话人停止。
                let client_id = u64::from(id.0);
                if let Some((name, _is_self)) = state.talking_stop(client_id) {
                    state.emit_talking(&app, client_id, name, false).await;
                }
            }
        }
    }
}

/// 电平推送任务：100ms 限频读原子量，发 voice://level（电平与概率，不含原始采样）。
/// 自己的说话状态：PTT 模式由发送开关推导，VAD 模式由门控推导（两模式互斥）。
pub async fn level_task(app: AppHandle, state: crate::app_state::AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    loop {
        interval.tick().await;
        if state.audio.active_conn() == 0 {
            continue;
        }
        // 自己：PTT 按住或 VAD 门开 ↔ TalkingState。
        let own = state.own_client();
        if own != 0 {
            let self_talking = state.audio.transmit.load(Ordering::Relaxed)
                || state.audio.vad_active.load(Ordering::Relaxed);
            if self_talking {
                if state.talking_start(own, state.self_nickname(), true) {
                    let name = state.self_nickname();
                    state.emit_talking(&app, own, name, true).await;
                } else {
                    state.talking_touch(own);
                }
            } else if let Some((name, _)) = state.talking_stop(own) {
                state.emit_talking(&app, own, name, false).await;
            }
        }
        // 兜底：>1.2s 无包视为停止（覆盖丢失的 TalkerStopped；松开后官方端
        // 停止显示的验收上限约 1s，由静音尾包+队列耗尽正常路径完成）。
        for (client_id, name, _) in state.talking_sweep(std::time::Duration::from_millis(1200)) {
            state.emit_talking(&app, client_id, name, false).await;
        }
        let mic = f32::from_bits(state.audio.mic_level.load(Ordering::Relaxed));
        let out = f32::from_bits(state.audio.out_level.load(Ordering::Relaxed));
        let prob = f32::from_bits(state.audio.vad_prob.load(Ordering::Relaxed));
        let _ = app.emit("voice://level", json!({ "mic": mic, "out": out, "prob": prob }));
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
    // M3 A3：界面按钮是双来源之一；全局热键由 hotkey::watch_task 写入另一来源。
    state.audio.set_button_ptt(enabled);
    Ok(())
}

/// B1 量纲锁定测试：nnnoiseless 契约要求 16-bit 量纲（-32768..32767 的 f32）。
/// 固定确定性素材（xorshift 噪声 + 谐波"人声等价"段）下：
/// 1) 噪声段 RMS 明显下降；2) 稳态人声段不被削平；
/// 3) 若用 -1..1 量纲喂入，同样的断言必须失败——证明量纲错误逃不过测试。
#[cfg(test)]
mod tests {
    use super::tests_util::measure_denoise;

    /// 稳态白噪声段：降噪后 RMS 与高频段至少下降约 1.4dB（比例 ≤ 0.85）。
    /// 实测 nnnoiseless 对宽带噪声 RMS 压制约 2.3dB、高频带约 3.3dB；
    /// 错误量纲下两者均 ≈1.0（完全惰性），间隔充足。
    const MIN_NOISE_DROP_RATIO: f32 = 0.85;
    /// 人声段峰值至少保留 50%（不被削平）。
    const MIN_VOICE_KEEP_RATIO: f32 = 0.5;

    #[test]
    fn denoise_at_16bit_scale_reduces_noise_and_keeps_voice() {
        let (noise_ratio, hp_ratio, voice_keep, noise_prob) = measure_denoise(32_768.0);
        assert!(
            noise_ratio <= MIN_NOISE_DROP_RATIO,
            "稳态噪声段 RMS 未明显下降：保留比例 {noise_ratio:.3}（要求 ≤ {MIN_NOISE_DROP_RATIO}）"
        );
        assert!(
            hp_ratio <= MIN_NOISE_DROP_RATIO,
            "噪声高频段未明显衰减：比例 {hp_ratio:.3}（要求 ≤ {MIN_NOISE_DROP_RATIO}）"
        );
        assert!(
            voice_keep >= MIN_VOICE_KEEP_RATIO,
            "稳态人声段被削平：峰值保留比例 {voice_keep:.3}（要求 ≥ {MIN_VOICE_KEEP_RATIO}）"
        );
        assert!(
            noise_prob >= 0.1,
            "降噪器对稳态噪声无响应（概率 {noise_prob:.3}），疑似错误量纲或模型未加载"
        );
    }

    #[test]
    fn denoise_at_wrong_scale_fails_the_same_assertions() {
        // 用 -1..1 量纲喂入：nnnoiseless 视其为远低于工作电平的极小 16-bit 信号，
        // 模型完全静默（概率=0、增益=1、噪声不衰减）。主测试的"噪声明显下降"
        // 断言在错误量纲下必然失败——量纲喂错必报警。
        let (noise_ratio_wrong, hp_ratio_wrong, _voice_keep_wrong, prob_wrong) =
            measure_denoise(1.0);
        assert!(
            prob_wrong < 0.05 && noise_ratio_wrong > 0.95 && hp_ratio_wrong > 0.95,
            "错误量纲（-1..1）下降噪器行为异常：prob={prob_wrong:.3} noise={noise_ratio_wrong:.3} hp={hp_ratio_wrong:.3}（应全部惰性）"
        );
        let (noise_ratio_right, _, voice_keep_right, _) = measure_denoise(32_768.0);
        assert!(
            noise_ratio_right <= MIN_NOISE_DROP_RATIO && voice_keep_right >= MIN_VOICE_KEEP_RATIO,
            "正确量纲未通过主断言（噪声比 {noise_ratio_right:.3}，人声保留 {voice_keep_right:.3}），素材或实现回归"
        );
    }
}

/// 测试工具模块（供 tests 引用的最小实现，避免触碰私有状态）。
#[cfg(test)]
pub mod tests_util {
    use nnnoiseless::DenoiseState;

    pub const FRAME: usize = 480;
    pub const NOISE_FRAMES: usize = 200;
    pub const VOICE_FRAMES: usize = 200;

    struct XorShift(u32);
    impl XorShift {
        fn next_f32(&mut self) -> f32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            x as i32 as f32 / 2_147_483_648.0
        }
    }

    /// 确定性素材：前半段稳态宽带噪声，后半段 440/880/1320Hz 谐波稳态"人声等价"段。
    /// 返回每帧 480 样本的 16-bit 量纲向量。
    fn material(scale: f32) -> Vec<[f32; FRAME]> {
        let mut rng = XorShift(0x1234_5678);
        let mut frames = Vec::with_capacity(NOISE_FRAMES + VOICE_FRAMES);
        let tau = std::f32::consts::TAU;
        let mut phase = [0f32; 3];
        for i in 0..NOISE_FRAMES + VOICE_FRAMES {
            let mut f = [0f32; FRAME];
            for s in f.iter_mut() {
                if i < NOISE_FRAMES {
                    *s = rng.next_f32() * 0.1; // 稳态宽带噪声（-1..1 量纲，≈-26dBFS RMS）
                } else {
                    let mut v = 0.0f32;
                    for (p, hz) in phase.iter_mut().zip([440.0, 880.0, 1320.0]) {
                        *p += hz * tau / 48_000.0;
                        if *p > tau {
                            *p -= tau;
                        }
                        v += p.sin();
                    }
                    *s = v / 3.0 * 0.3; // 稳态谐波"人声"段（峰值 ≈0.3）
                }
            }
            frames.push(f.map(|v| (v * scale).clamp(-32_768.0, 32_767.0)));
        }
        frames
    }

    /// 逐帧 process_frame（16-bit 量纲），丢弃首帧淡入与收敛期；
    /// 返回（噪声段全带 RMS 比例，噪声段高频比比例，人声段峰值保留比例，噪声段语音概率均值）。
    pub fn measure_denoise(scale: f32) -> (f32, f32, f32, f32) {
        let frames = material(scale);
        let mut denoiser = DenoiseState::new();
        let mut out = [0f32; FRAME];
        let mut dbg_prob_n = 0f32;
        let mut hp_in_rms = 0f32;
        let mut hp_out_rms = 0f32;
        let mut hp_cnt = 0f32;
        let mut noise_in_rms = 0f32;
        let mut noise_out_rms = 0f32;
        let mut voice_in_peak = 0f32;
        let mut voice_out_peak = 0f32;
        // 统计跳过段：首帧淡入 + 前 50 帧模型收敛期，其余为稳态。
        for (i, frame) in frames.iter().enumerate() {
            let prob = denoiser.process_frame(&mut out, frame);
            let _ = prob;
            let converged = i >= 50;
            let rms_in = (frame.iter().map(|v| v * v).sum::<f32>() / FRAME as f32).sqrt();
            let rms_out = (out.iter().map(|v| v * v).sum::<f32>() / FRAME as f32).sqrt();
            let peak_in = frame.iter().fold(0f32, |m, v| m.max(v.abs()));
            let peak_out = out.iter().fold(0f32, |m, v| m.max(v.abs()));
            if !converged {
                continue;
            }
            if i < NOISE_FRAMES {
                dbg_prob_n += prob;
                noise_in_rms += rms_in * rms_in;
                noise_out_rms += rms_out * rms_out;
                // 高频段（一阶差分 ≈ 高通）能量：宽带噪声的主要成分，
                // 降噪器压制最狠，量纲也最敏感。
                let mut din = [0f32; FRAME];
                let mut dout = [0f32; FRAME];
                for n in 1..FRAME {
                    din[n] = frame[n] - frame[n - 1];
                    dout[n] = out[n] - out[n - 1];
                }
                hp_in_rms += (din.iter().map(|v| v * v).sum::<f32>() / FRAME as f32).sqrt();
                hp_out_rms += (dout.iter().map(|v| v * v).sum::<f32>() / FRAME as f32).sqrt();
                hp_cnt += 1.0;
            } else {
                voice_in_peak = voice_in_peak.max(peak_in);
                voice_out_peak = voice_out_peak.max(peak_out);
            }
        }
        let noise_in = (noise_in_rms / (NOISE_FRAMES - 1) as f32).sqrt();
        let noise_out = (noise_out_rms / (NOISE_FRAMES - 1) as f32).sqrt();
        let noise_ratio = if noise_in > 0.0 { noise_out / noise_in } else { 1.0 };
        let voice_keep = if voice_in_peak > 0.0 { voice_out_peak / voice_in_peak } else { 0.0 };
        let hp_ratio = if hp_in_rms > 0.0 { (hp_out_rms / hp_cnt) / (hp_in_rms / hp_cnt) } else { 1.0 };
        let noise_prob = dbg_prob_n / (NOISE_FRAMES - 50) as f32;
        (noise_ratio, hp_ratio, voice_keep, noise_prob)
    }
}
