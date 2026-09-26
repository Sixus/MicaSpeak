use crate::app_state::{ActiveConnection, AppState, ConnOwner};
use crate::audio::{send_task, SEND_QUEUE};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, oneshot};
use tsclientlib::prelude::*;
use tsclientlib::sync::{SyncConnection, SyncConnectionHandle, SyncStreamItem};
use tsclientlib::{
    Connection, DisconnectOptions, Error as TslError, Identity, TemporaryDisconnectReason, TsError,
};
use tsproto_packets::packets::AudioData;
use tracing::{debug, info, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionPayload {
    pub status: String,
    pub reason: Option<String>,
    pub server_name: Option<String>,
    pub server_address: Option<String>,
}
impl Default for ConnectionPayload {
    fn default() -> Self {
        Self {
            status: "disconnected".into(),
            reason: None,
            server_name: None,
            server_address: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientNode {
    pub id: u64,
    pub name: String,
    pub channel_id: u64,
    pub is_self: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub order: u64,
    pub password: bool,
    pub clients: Vec<ClientNode>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub address: String,
    pub nickname: String,
    pub password_saved: bool,
    pub last_channel: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub connection: ConnectionPayload,
    pub channels: Vec<ChannelNode>,
    pub bookmarks: Vec<Bookmark>,
    pub last_channel: Option<String>,
    pub runtime_available: bool,
    pub talking: Vec<crate::app_state::TalkerState>,
    pub voice: crate::persistence::VoiceSettingsView,
    pub own_channel_id: u64,
    pub chat: Vec<crate::chat::ChatTabView>,
    pub overlay_enabled: bool,
    /// M5a 窗口材质："Mica" / "实体（回退：…）" / "实体（--force-fallback）"。
    pub material: String,
    /// M5c Evergreen WebView2 版本（缺省 None；向后兼容新字段）。
    pub webview2_version: Option<String>,
    /// M6a 身份列表（脱敏视图：id/昵称/脱敏 uid/等级/是否当前）。
    pub identities: Vec<crate::identity::IdentityView>,
}

pub enum ConnCommand {
    SelectChannel {
        id: u64,
        password: Option<String>,
        result: oneshot::Sender<Result<(), String>>,
    },
    SendChannelMessage {
        text: String,
        result: oneshot::Sender<Result<(), String>>,
    },
    SendPrivateMessage {
        client_id: u64,
        text: String,
        result: oneshot::Sender<Result<(), String>>,
    },
    Shutdown,
}

/// Evergreen WebView2 Runtime 探测结果（M5c：缺失时给出前置条件页并记录
/// 版本/安装来源；不得把缺失 Runtime 误报为连接错误）。
#[derive(Clone, Debug)]
pub struct WebView2Info {
    pub available: bool,
    pub version: Option<String>,
    /// "HKLM（系统级）" | "HKCU（当前用户）"
    pub source: Option<String>,
}

fn probe_hive(root: windows::Win32::System::Registry::HKEY, full_key: &str) -> Option<String> {
    crate::registry::read_string(root, full_key, "pv")
}

/// Evergreen Runtime 的安装项 GUID（微软官方检测路径，docs R2）。
const WEBVIEW2_EVERGREEN_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

pub fn webview2_probe() -> WebView2Info {
    if std::env::var_os("MICASPEAK_FORCE_WEBVIEW2_MISSING").is_some() {
        return WebView2Info { available: false, version: None, source: None };
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        // 直查固定 GUID 子键（不递归、零进程派生）：EdgeUpdate\Clients 树很大，
        // reg.exe /s 实测 6 秒；GUI 进程派生 reg.exe 在云桌面上又叠加控制台
        // 创建开销（每次 3-6 秒）——全部走 RegGetValueW 直读（M5c 指标修正）。
        for (source, root, key) in [
            (
                "HKLM（系统级）",
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
            ),
            (
                "HKCU（当前用户）",
                HKEY_CURRENT_USER,
                r"Software\Microsoft\EdgeUpdate\Clients",
            ),
        ] {
            let full = format!("{key}\\{WEBVIEW2_EVERGREEN_GUID}");
            if let Some(version) = probe_hive(root, &full) {
                return WebView2Info {
                    available: true,
                    version: Some(version),
                    source: Some(source.into()),
                };
            }
        }
        WebView2Info { available: false, version: None, source: None }
    }
    #[cfg(not(windows))]
    {
        WebView2Info { available: true, version: None, source: None }
    }
}

/// 把协议库错误转成可直接展示的中文；不包含密码等敏感内容。
fn friendly_error(e: &TslError) -> String {
    match e {
        TslError::CommandError(ce) => match ce.error {
            TsError::ChannelInvalidPassword => "频道密码错误".into(),
            TsError::ServerInvalidPassword => "服务器密码错误".into(),
            TsError::PermissionsClientInsufficient => "权限不足，无法执行此操作".into(),
            ref other => format!("操作失败：{other:?}"),
        },
        TslError::ConnectTs(e) => match e {
            TsError::ServerInvalidPassword => "服务器密码错误".into(),
            ref other => format!("服务器拒绝连接：{other:?}"),
        },
        TslError::ConnectFailed { .. }
        | TslError::Connect(_)
        | TslError::ConnectionFailed(_)
        | TslError::ResolveAddress(_)
        | TslError::InitserverTimeout
        | TslError::InitserverWait(_) => {
            "无法连接服务器，请检查地址和端口是否正确，以及网络是否可用".into()
        }
        other => format!("连接错误：{other}"),
    }
}

#[tauri::command]
pub async fn get_app_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    // M5c 冷启动指标：前端拿到首屏数据 = 可交互起点（只记一次）。
    crate::logging::log_first_frame_ready();
    let snap = state.snapshot().await;
    if let Some(message) = state.take_pending_error().await {
        let _ = app.emit("error://user", serde_json::json!({ "message": message }));
    }
    Ok(snap)
}

#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    address: String,
    nickname: String,
    password: Option<String>,
    bookmark_id: Option<String>,
) -> Result<(), String> {
    // 用户主动连接：epoch+1 取消任何挂起的自动重连（B2 手动优先）。
    state.reconnect_epoch.fetch_add(1, Ordering::Relaxed);
    let shared = state.inner().clone();
    connect_inner(&app, &shared, ActiveConnection {
        address,
        nickname,
        password: password.filter(|p| !p.is_empty()),
        bookmark_id,
    }, None)
    .await
}

/// 连接入口（connect 命令与自动重连共用）。`rejoin_override` 非空时直接作为
/// 连上后的自动进频道目标（自动重连用它恢复断开前的频道；书签路径自带）。
async fn connect_inner(
    app: &AppHandle,
    state: &AppState,
    active: ActiveConnection,
    rejoin_override: Option<String>,
) -> Result<(), String> {
    let ActiveConnection { address, nickname, password, bookmark_id } = active;
    // 新连接任务必须清理旧任务（docs/09 B2）：旧 owner 收到 Shutdown，
    // 音频流/说话状态/频道树全部复位。
    disconnect_inner(state).await;
    // M6a：加载当前身份（私钥只在 Rust 内存中短暂存在，不进事件/日志）。
    let identity = crate::identity::load_active_identity(&state.config)
        .await
        .map_err(|e| format!("身份无法读取：{e}"))?
        .identity;
    let mut password = password.filter(|p| !p.is_empty());
    let mut auto_join = rejoin_override;
    if auto_join.is_none() {
        if let Some(id) = bookmark_id.as_deref() {
            let cfg = state.config.lock().await.clone();
            if let Some(bookmark) = cfg.bookmarks.iter().find(|b| b.id == id) {
                if password.is_none() {
                    password = bookmark.password.clone().filter(|p| !p.is_empty());
                }
                auto_join = bookmark.last_channel.clone();
            }
        }
    }
    debug!(?bookmark_id, ?auto_join, "connect: bookmark resolved");
    state.set_self_nickname(nickname.clone());
    *state.active.lock().await = Some(ActiveConnection {
        address: address.clone(),
        nickname: nickname.clone(),
        password: password.clone(),
        bookmark_id,
    });
    {
        let mut payload = state.connection.lock().await;
        payload.status = "connecting".into();
        payload.reason = None;
        payload.server_address = Some(address.clone());
    }
    let _ = app.emit("connection://state", state.connection.lock().await.clone());
    // 立刻推送快照，让前端马上进入“连接中”状态（否则真实连接过程 UI 无反馈）
    state.emit_snapshot(app).await;
    let id = state.next_conn_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = mpsc::channel(8);
    *state.conn_tx.lock().await = Some(ConnOwner { id, tx });
    let shared = state.clone();
    let app2 = app.clone();
    // dyn 擦除：打断 connect_inner→connection_loop→driver_loop→arm_reconnect→
    // connect_inner 的 future 类型级循环（E0391，Box::pin 不足以擦除类型）。
    tauri::async_runtime::spawn(Box::pin(connection_loop(
        app2,
        shared,
        rx,
        id,
        address,
        nickname,
        password,
        identity,
        auto_join,
    )) as Pin<Box<dyn Future<Output = ()> + Send>>);
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    // 用户主动断开：epoch+1 取消挂起的自动重连（B2：手动断开不重连）。
    state.reconnect_epoch.fetch_add(1, Ordering::Relaxed);
    info!("用户手动断开连接");
    disconnect_inner(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn reconnect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let active = state.active.lock().await.clone();
    if let Some(a) = active {
        return connect(app, state, a.address, a.nickname, a.password, a.bookmark_id).await;
    }
    let cfg = state.config.lock().await.clone();
    let b = cfg
        .bookmarks
        .first()
        .ok_or_else(|| "没有可重连的书签".to_string())?
        .clone();
    connect(app, state, b.address, b.nickname, b.password, Some(b.id)).await
}

#[tauri::command]
pub async fn select_channel(
    state: State<'_, AppState>,
    channel_id: u64,
    password: Option<String>,
) -> Result<(), String> {
    let tx = state
        .conn_tx
        .lock()
        .await
        .as_ref()
        .map(|o| o.tx.clone())
        .ok_or_else(|| "当前没有活动连接".to_string())?;
    let (result_tx, result_rx) = oneshot::channel();
    tx.send(ConnCommand::SelectChannel {
        id: channel_id,
        password,
        result: result_tx,
    })
    .await
    .map_err(|_| "连接任务已结束".to_string())?;
    result_rx.await.map_err(|_| "连接任务已结束".to_string())?
}

async fn disconnect_inner(state: &AppState) {
    if let Some(owner) = state.conn_tx.lock().await.take() {
        let _ = owner.tx.send(ConnCommand::Shutdown).await;
    }
    state.audio.stop();
    state.talking_clear();
    state.set_own_client(0);
    // 聊天历史运行内保留（F3），只重置"当前频道/正在查看/已关闭"等会话事实。
    state.chat.reset_session();
    *state.channels.lock().await = Vec::new();
    *state.connection.lock().await = ConnectionPayload::default();
}

/// 只有当前连接仍然属于自己时才清理共享状态，避免旧任务清掉新连接。
/// 注意：不重置 connection payload——失败原因由 publish_error 写入，
/// 是连接的最终状态，必须保留给用户看。
async fn cleanup_if_current(state: &AppState, id: u64) {
    if is_current(state, id).await {
        *state.conn_tx.lock().await = None;
        *state.channels.lock().await = Vec::new();
        state.audio.stop();
        state.talking_clear();
        state.set_own_client(0);
    }
}

async fn is_current(state: &AppState, id: u64) -> bool {
    state.conn_tx.lock().await.as_ref().map(|o| o.id) == Some(id)
}

/// 连接任务：命令循环只通过 handle 操作连接；
/// 真正驱动事件流的轮询任务见 driver_loop。
/// 注意：绝不能在轮询流的同一条任务里 await handle 操作——
/// handle 调用的闭包要等下一次流轮询才执行，会造成自死锁
/// （M1 期间"进入频道后 ~25 秒掉线"的根因）。
async fn connection_loop(
    app: AppHandle,
    state: AppState,
    mut rx: mpsc::Receiver<ConnCommand>,
    id: u64,
    address: String,
    nickname: String,
    password: Option<String>,
    identity: Identity,
    auto_join: Option<String>,
) {
    let options = Connection::build(address.clone())
        .name(nickname)
        .identity(identity);
    let options = if let Some(password) = password {
        options.password(password)
    } else {
        options
    };
    let connection = match options.connect() {
        Ok(c) => c,
        Err(e) => {
            publish_error(&app, &state, friendly_error(&e), Some(address)).await;
            cleanup_if_current(&state, id).await;
            return;
        }
    };
    let sync: SyncConnection = connection.into();
    let mut handle = sync.get_handle();

    // 音频发送任务：独立 handle + 独立任务，等待编码包并转发（音频流在
    // publish_state 确认 connected 后才创建，见 AudioManager.ensure_started）。
    let audio_handle = sync.get_handle();
    let (audio_tx, audio_rx) = mpsc::channel(SEND_QUEUE);
    tauri::async_runtime::spawn(send_task(audio_handle, audio_rx));
    state.audio.register_connection(id, audio_tx);
    // M3：连接建立时用 config 里的语音参数初始化音频管理器（模式/阈值/降噪/设备）。
    {
        let config = state.config.lock().await;
        let v = &config.voice;
        state.audio.set_voice_params(crate::audio::VoiceParams {
            input_device: v.input_device.clone(),
            output_device: v.output_device.clone(),
        });
        // 模式/阈值/降噪是原子量：连接时从配置同步一次，之后由设置命令即时改。
        state.audio.sync_vad_controls(v.mode == "vad", v.vad_threshold, v.denoise);
    }

    // 事件驱动任务：独占轮询事件流，负责发布状态与错误。
    let driver_app = app.clone();
    let driver_state = state.clone();
    let driver_address = address.clone();
    let auto_tx = state
        .conn_tx
        .lock()
        .await
        .as_ref()
        .map(|o| o.tx.clone());
    tauri::async_runtime::spawn(driver_loop(
        driver_app,
        driver_state,
        sync,
        driver_address,
        auto_tx,
        auto_join,
        id,
    ));

    // 命令循环：处理前端命令，经 handle 操作连接（驱动任务保持轮询）。
    while let Some(command) = rx.recv().await {
        match command {
            ConnCommand::Shutdown => {
                let _ = handle.disconnect(DisconnectOptions::new()).await;
                break;
            }
            ConnCommand::SelectChannel {
                id: channel_id,
                password,
                result,
            } => {
                let res = move_to_channel(&mut handle, channel_id, password).await;
                let _ = result.send(res.clone());
                match res {
                    Ok(()) => save_last_channel(&app, &state, channel_id).await,
                    Err(message) => {
                        let _ = app.emit(
                            "error://user",
                            serde_json::json!({ "message": message }),
                        );
                    }
                }
            }
            ConnCommand::SendChannelMessage { text, result } => {
                let res = send_channel_message_inner(&app, &state, &mut handle, text).await;
                let _ = result.send(res.clone());
                if let Err(message) = res {
                    let _ = app.emit(
                        "error://user",
                        serde_json::json!({ "message": message }),
                    );
                }
            }
            ConnCommand::SendPrivateMessage { client_id, text, result } => {
                let res = send_private_message_inner(&app, &state, &mut handle, client_id, text).await;
                let _ = result.send(res.clone());
                if let Err(message) = res {
                    let _ = app.emit(
                        "error://user",
                        serde_json::json!({ "message": message }),
                    );
                }
            }
        }
    }
}

/// 独占轮询事件流：转发状态/错误事件，并在首次连上后自动回连上次频道。
/// 额外职责（M5b B2）：
/// - 记录自己当前所在频道名 `joined`；库内临时断连自动重建后，`pending_rejoin`
///   恢复原频道（书签路径之外的覆盖：非书签连接、连接后换过频道的情况）。
/// - 流意外结束（网络死亡且库内重建失败）时按退避序列发起应用层自动重连；
///   用户手动操作（epoch 变化）则不重连。
async fn driver_loop(
    app: AppHandle,
    state: AppState,
    mut sync: SyncConnection,
    address: String,
    cmd_tx: Option<mpsc::Sender<ConnCommand>>,
    mut auto_join: Option<String>,
    id: u64,
) {
    let mut joined: Option<String> = None;
    let mut pending_rejoin: Option<String> = None;
    while let Some(item) = sync.next().await {
        // 连接已被替换/断开（owner 换人）后不再发布任何状态，
        // 否则会把 disconnect 重置好的界面覆盖成幽灵"已连接"。
        if !is_current(&state, id).await {
            break;
        }
        match item {
            Ok(SyncStreamItem::BookEvents(events)) => {
                publish_state(&app, &state, &mut sync, &address, id).await;
                // 刷新"自己所在频道"（重连恢复与临时断连重进的依据）。
                let own = state.chat.own_channel();
                if own != 0 {
                    if let Some(name) = state
                        .channels
                        .lock()
                        .await
                        .iter()
                        .find(|c| c.id == own)
                        .map(|c| c.name.clone())
                    {
                        joined = Some(name);
                    }
                }
                // 文字消息与 book 变更同批到达；先刷新 own_channel 再路由，
                // 保证消息归入正确频道标签（任务卡 A3）。
                for event in events {
                    if let tsclientlib::events::Event::Message { target, invoker, message } = event {
                        handle_incoming_text(&app, &state, target, invoker, message).await;
                    }
                }
                // 进频道目标：auto_join（书签/重连注入）优先，其次库内临时断连
                // 重建后的重进。频道列表未就绪时保留目标等下一次事件重试，
                // 找到（含密码频道）才消费掉。
                let mut from_auto_join = false;
                let target = if auto_join.is_some() {
                    from_auto_join = true;
                    auto_join.clone()
                } else {
                    pending_rejoin.clone()
                };
                if let Some(name) = target {
                    let find = state
                        .channels
                        .lock()
                        .await
                        .iter()
                        .find(|c| c.name == name)
                        .map(|c| (c.id, c.password));
                    debug!(name = %name, ?find, "driver: join target");
                    match find {
                        Some((channel_id, false)) => {
                            auto_join = None;
                            pending_rejoin = None;
                            if let Some(cmd_tx) = &cmd_tx {
                                let (result, _) = oneshot::channel();
                                let _ = cmd_tx
                                    .clone()
                                    .send(ConnCommand::SelectChannel {
                                        id: channel_id,
                                        password: None,
                                        result,
                                    })
                                    .await;
                            }
                        }
                        Some((_, true)) => {
                            // 密码频道没有已存密码，静默跳过（设计内：用户手动输入）
                            auto_join = None;
                            pending_rejoin = None;
                        }
                        None => {
                            // 未就绪：auto_join 保留重试语义；pending_rejoin 同样
                            // 保留（频道树马上会到）。
                            if from_auto_join {
                                auto_join = Some(name);
                            }
                        }
                    }
                }
            }
            Ok(SyncStreamItem::Audio(packet)) => {
                // B1：收到的音频包喂给播放队列（分说话人/重排/PLC/混音在库内完成）。
                let from = match packet.data().data() {
                    AudioData::S2C { from, .. } => *from,
                    AudioData::S2CWhisper { from, .. } => *from,
                    _ => continue,
                };
                // C1 说话起点：库未暴露 talker 事件流，取 AudioHandler::handle_packet
                // 的返回值（新队列=开始说话）；结束由播放回调 TalkerStopped + 电平任务
                // 兜底清扫推导（任务卡允许的方案，来源已在卡 C1 注明）。
                if let Some(new) = state.audio.play_packet(from, packet) {
                    let new_id = u64::from(new.0);
                    let name = state
                        .client_name(new_id)
                        .await
                        .unwrap_or_else(|| format!("用户 {new_id}"));
                    if state.talking_start(new_id, name.clone(), false) {
                        state.emit_talking(&app, new_id, name, true).await;
                    }
                } else {
                    state.talking_touch(u64::from(from));
                }
            }
            Ok(SyncStreamItem::DisconnectedTemporarily(reason)) => {
                // 库内自动重建即将开始；记住当前频道，重建成功后重进。
                if pending_rejoin.is_none() {
                    pending_rejoin = joined.clone();
                }
                info!(?reason, ?pending_rejoin, "临时断连：库内将自动重建，准备重进原频道");
                publish_temporary_disconnect(&app, &state, reason, address.clone()).await;
            }
            Ok(_) => {}
            Err(e) => {
                publish_error(&app, &state, friendly_error(&e), Some(address.clone())).await;
                break;
            }
        }
    }
    // 流结束时的归属判定：
    // - 仍持 owner（is_current）= 流意外死亡（网络断且库内重建失败）→ 退避重连；
    //   用户手动断开/换连接会先取走 owner，走不到这里（B2：手动断开不重连）。
    // - owner 已被取走 = 用户操作，旧任务静默收尾。
    if is_current(&state, id).await {
        state.audio.stop();
        state.talking_clear();
        state.set_own_client(0);
        state.chat.reset_session();
        arm_reconnect(app, state, address).await;
    } else {
        cleanup_if_current(&state, id).await;
        state.emit_snapshot(&app).await;
    }
}

/// 退避序列（秒）：5/10/20/40，之后封顶 60（docs/09 B2）。
const RECONNECT_DELAYS: [u64; 5] = [5, 10, 20, 40, 60];

fn backoff_delay(attempt: usize) -> u64 {
    RECONNECT_DELAYS[(attempt - 1).min(RECONNECT_DELAYS.len() - 1)]
}

/// 经函数指针调用 connect_inner：arm_reconnect 的协程类型里只出现具体的
/// fn 指针类型，不再内嵌 connect_inner 的 opaque future，从而打断
/// connect_inner→connection_loop→driver_loop→arm_reconnect→connect_inner
/// 的类型级循环（E0391）。
fn connect_via_ptr(
    app: AppHandle,
    state: AppState,
    active: ActiveConnection,
    rejoin: Option<String>,
) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
    Box::pin(async move { connect_inner(&app, &state, active, rejoin).await })
}

/// 等待 wait_secs 秒；期间用户手动连接/断开（epoch 变化）立即返回 false。
async fn wait_epoch(state: &AppState, wait_secs: u64, epoch: u64) -> bool {
    for _ in 0..wait_secs {
        if state.reconnect_epoch.load(Ordering::Relaxed) != epoch {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    state.reconnect_epoch.load(Ordering::Relaxed) == epoch
}

/// 应用层自动重连（docs/09 B2）：网络死亡（流意外结束）后按 5/10/20/40/60 秒
/// 退避重连；连接参数复用断开前的 active，频道用 config.last_channel 恢复
/// （select_channel 成功时会同步写回，见 save_last_channel）。恢复音频流由
/// publish_state → ensure_started 完成。用户随时可手动连接/断开接管（epoch）。
async fn arm_reconnect(app: AppHandle, state: AppState, address: String) {
    let epoch = state.reconnect_epoch.load(Ordering::Relaxed);
    tauri::async_runtime::spawn(async move {
        let mut attempt: usize = 0;
        loop {
            attempt += 1;
            let delay = backoff_delay(attempt);
            warn!("连接意外断开（{address}），{delay} 秒后进行第 {attempt} 次自动重连");
            {
                let payload = ConnectionPayload {
                    status: "disconnected".into(),
                    reason: Some(format!("连接意外断开，{delay} 秒后自动重连（第 {attempt} 次）")),
                    server_name: None,
                    server_address: Some(address.clone()),
                };
                *state.connection.lock().await = payload.clone();
                let _ = app.emit("connection://state", payload);
                state.emit_snapshot(&app).await;
            }
            if !wait_epoch(&state, delay, epoch).await {
                info!("自动重连已取消：用户手动操作接管");
                return;
            }
            let Some(active) = state.active.lock().await.clone() else {
                warn!("自动重连中止：没有可复用的连接参数");
                return;
            };
            // 频道恢复目标：断开前所在频道（save_last_channel 已持久化）。
            let rejoin = state.config.lock().await.last_channel.clone();
            info!("自动重连：第 {attempt} 次尝试 → {}（恢复频道 {rejoin:?}）", active.address);
            if let Err(e) = connect_via_ptr(app.clone(), state.clone(), active, rejoin).await {
                warn!("自动重连第 {attempt} 次发起失败：{e}");
                continue;
            }
            // 等本次尝试出结果：connected=成功返回；disconnected=失败继续退避。
            loop {
                if state.reconnect_epoch.load(Ordering::Relaxed) != epoch {
                    info!("自动重连已取消：用户手动操作接管");
                    return;
                }
                let payload = state.connection.lock().await.clone();
                match payload.status.as_str() {
                    "connected" => {
                        info!("自动重连成功（第 {attempt} 次尝试）：频道与音频已恢复");
                        return;
                    }
                    "disconnected" => {
                        warn!(
                            "自动重连第 {attempt} 次失败：{}",
                            payload.reason.unwrap_or_else(|| "未知原因".into())
                        );
                        break;
                    }
                    _ => {}
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    });
}

async fn publish_state(
    app: &AppHandle,
    state: &AppState,
    sync: &mut SyncConnection,
    address: &str,
    id: u64,
) {
    // 驱动任务独占轮询，可直接解引用读取连接数据（不经过 handle，避免死锁）
    let Some(book) = sync.get_state().ok() else {
        return;
    };
    let server_name = book.server.name.clone();
    let own_client = book.own_client;
    state.set_own_client(u64::from(own_client.0));
    let channels = book
        .channels
        .values()
        .map(|c| (c.id.0, c.parent.0, c.name.clone(), c.order.0, c.has_password))
        .collect::<Vec<_>>();
    let clients = book
        .clients
        .values()
        .map(|c| (c.id.0, c.name.clone(), c.channel.0))
        .collect::<Vec<_>>();

    let nodes = channels
        .into_iter()
        .map(|(id, parent, name, order, password)| ChannelNode {
            id,
            parent_id: (parent != 0).then_some(parent),
            name,
            order,
            password: password.unwrap_or(false),
            clients: clients
                .iter()
                .filter(|(_, _, channel)| *channel == id)
                .map(|(cid, name, _)| ClientNode {
                    id: u64::from(*cid),
                    name: name.clone(),
                    channel_id: id,
                    is_self: *cid as u16 == own_client.0,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    // 真正 connected：此刻起允许音频发送流存在（A1 生命周期起点）。
    state.audio.ensure_started(id);
    // 当前频道：频道消息路由与未读判定的依据（publish 先于同批消息处理）。
    let own_channel = clients
        .iter()
        .find(|(cid, _, _)| *cid as u16 == own_client.0)
        .map(|(_, _, channel)| *channel)
        .unwrap_or(0);
    state.chat.set_own_channel(own_channel);
    *state.connection.lock().await = ConnectionPayload {
        status: "connected".into(),
        reason: None,
        server_name: Some(server_name),
        server_address: Some(address.to_string()),
    };
    *state.channels.lock().await = nodes.clone();
    let _ = app.emit("connection://state", state.connection.lock().await.clone());
    let _ = app.emit("channel://tree", nodes);
    state.emit_snapshot(app).await;
}

/// 收到的文字消息路由（任务卡 A3：按 id，不按昵称）。
/// 频道消息协议不携带频道 id，归属=自己当前所在频道；
/// invoker 为自己说明服务器回显了已本地落账的消息，跳过去重。
async fn handle_incoming_text(
    app: &AppHandle,
    state: &AppState,
    target: tsclientlib::MessageTarget,
    invoker: tsclientlib::Invoker,
    message: String,
) {
    let invoker_id = u64::from(invoker.id.0);
    if invoker_id == state.own_client() {
        return;
    }
    match target {
        tsclientlib::MessageTarget::Channel => {
            let channel_id = state.chat.own_channel();
            let title = state
                .channels
                .lock()
                .await
                .iter()
                .find(|c| c.id == channel_id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let update = state.chat.receive(
                crate::chat::ChatKey::Channel(channel_id),
                &title,
                invoker_id,
                invoker.name,
                message,
            );
            state.emit_chat_update(app, &update);
        }
        tsclientlib::MessageTarget::Client(_) => {
            // 私聊按发送者 client_id 路由（任务卡 B2）；标题取对方当前昵称。
            let name = state
                .channels
                .lock()
                .await
                .iter()
                .flat_map(|c| c.clients.iter())
                .find(|c| c.id == invoker_id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let update = state.chat.receive(
                crate::chat::ChatKey::Client(invoker_id),
                &name,
                invoker_id,
                invoker.name,
                message,
            );
            state.emit_chat_update(app, &update);
        }
        _ => {}
    }
}

/// 频道消息发送：服务器回执成功才本地落账并广播。
async fn send_channel_message_inner(
    app: &AppHandle,
    state: &AppState,
    handle: &mut SyncConnectionHandle,
    text: String,
) -> Result<(), String> {
    // with_connection 的闭包要求 'static，单独克隆一份文本进去
    let text_for_packet = text.clone();
    let packet = handle
        .with_connection(move |con| {
            con.get_state()
                .map(|book| book.send_message(tsclientlib::MessageTarget::Channel, &text_for_packet))
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("发送失败：{e}"))?;
    handle
        .send_command(packet)
        .await
        .map_err(|e| friendly_error(&e))?;
    let channel_id = state.chat.own_channel();
    let title = state
        .channels
        .lock()
        .await
        .iter()
        .find(|c| c.id == channel_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let own_client = state.own_client();
    let nickname = state.self_nickname();
    let update = state.chat.record_self(
        crate::chat::ChatKey::Channel(channel_id),
        &title,
        own_client,
        nickname,
        text,
    );
    state.emit_chat_update(app, &update);
    Ok(())
}

/// 私聊消息发送：按对方 client_id 路由，服务器回执成功才本地落账并广播。
/// 自己发起会同时打开该私聊标签（open 事实，任务卡 B1）。
async fn send_private_message_inner(
    app: &AppHandle,
    state: &AppState,
    handle: &mut SyncConnectionHandle,
    client_id: u64,
    text: String,
) -> Result<(), String> {
    // with_connection 的闭包要求 'static，单独克隆一份文本进去
    let text_for_packet = text.clone();
    let packet = handle
        .with_connection(move |con| {
            con.get_state()
                .map(|book| {
                    book.send_message(
                        tsclientlib::MessageTarget::Client(tsclientlib::ClientId(client_id as u16)),
                        &text_for_packet,
                    )
                })
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("发送失败：{e}"))?;
    handle
        .send_command(packet)
        .await
        .map_err(|e| friendly_error(&e))?;
    let name = state
        .channels
        .lock()
        .await
        .iter()
        .flat_map(|c| c.clients.iter())
        .find(|c| c.id == client_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    state.chat.open_private(client_id);
    let own_client = state.own_client();
    let nickname = state.self_nickname();
    let update = state.chat.record_self(
        crate::chat::ChatKey::Client(client_id),
        &name,
        own_client,
        nickname,
        text,
    );
    state.emit_chat_update(app, &update);
    Ok(())
}

async fn move_to_channel(
    handle: &mut SyncConnectionHandle,
    id: u64,
    password: Option<String>,
) -> Result<(), String> {
    let id = tsclientlib::ChannelId(id as u64);
    let client_id = handle
        .with_connection(|connection| connection.get_state().map(|book| book.own_client))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let packet = tsclientlib::messages::c2s::OutClientMovePart {
        client_id,
        channel_id: id,
        channel_password: password.map(Into::into),
    };
    handle
        .send_command(packet.to_packet())
        .await
        .map_err(|e| friendly_error(&e))
}

/// 进入频道成功后把频道名写入 config（原子写盘），并同时记录到对应书签。
async fn save_last_channel(app: &AppHandle, state: &AppState, channel_id: u64) {
    let name = state
        .channels
        .lock()
        .await
        .iter()
        .find(|c| c.id == channel_id)
        .map(|c| c.name.clone());
    let Some(name) = name else { return };
    let active = state.active.lock().await.clone();
    let mut config = state.config.lock().await;
    config.last_channel = Some(name.clone());
    if let Some(bookmark) = config.bookmarks.iter_mut().find(|b| {
        active.as_ref().is_some_and(|a| {
            a.bookmark_id.as_deref() == Some(b.id.as_str()) || a.address == b.address
        })
    }) {
        bookmark.last_channel = Some(name);
    }
    if let Err(e) = crate::persistence::save_config(&config) {
        let _ = app.emit(
            "error://user",
            serde_json::json!({ "message": format!("保存配置失败：{e}") }),
        );
    }
}

async fn publish_temporary_disconnect(
    app: &AppHandle,
    state: &AppState,
    reason: TemporaryDisconnectReason,
    address: String,
) {
    let detail = match reason {
        TemporaryDisconnectReason::Serverstop => "服务器已关闭".to_string(),
        TemporaryDisconnectReason::Timeout(_) => "网络超时".to_string(),
    };
    let payload = ConnectionPayload {
        status: "disconnected".into(),
        reason: Some(format!("连接暂时断开（{detail}），正在尝试恢复…")),
        server_name: None,
        server_address: Some(address),
    };
    *state.connection.lock().await = payload.clone();
    let _ = app.emit("connection://state", payload);
    state.emit_snapshot(app).await;
}

async fn publish_error(app: &AppHandle, state: &AppState, reason: String, address: Option<String>) {
    let payload = ConnectionPayload {
        status: "disconnected".into(),
        reason: Some(reason.clone()),
        server_name: None,
        server_address: address,
    };
    *state.connection.lock().await = payload.clone();
    let _ = app.emit("connection://state", payload);
    let _ = app.emit("error://user", serde_json::json!({ "message": reason }));
    state.emit_snapshot(app).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M5b B2：退避序列固定为 5/10/20/40 秒，第 5 次起封顶 60 秒。
    /// 意外断开后按此序列自动重连；用户手动操作（epoch）随时取消。
    #[test]
    fn backoff_sequence_matches_task_card() {
        let expected: Vec<u64> = vec![5, 10, 20, 40, 60, 60, 60, 60, 60, 60];
        for (attempt, want) in expected.iter().enumerate() {
            assert_eq!(
                backoff_delay(attempt + 1),
                *want,
                "第 {} 次重连的退避应为 {} 秒",
                attempt + 1,
                want
            );
        }
    }
}
