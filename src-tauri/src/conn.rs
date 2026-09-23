use crate::app_state::{ActiveConnection, AppState, ConnOwner};
use crate::audio::{send_task, SEND_QUEUE};
use crate::persistence::load_identity;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, oneshot};
use tsclientlib::prelude::*;
use tsclientlib::sync::{SyncConnection, SyncConnectionHandle, SyncStreamItem};
use tsclientlib::{
    Connection, DisconnectOptions, Error as TslError, Identity, TemporaryDisconnectReason, TsError,
};
use tsproto_packets::packets::AudioData;
use tracing::debug;

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
}

pub enum ConnCommand {
    SelectChannel {
        id: u64,
        password: Option<String>,
        result: oneshot::Sender<Result<(), String>>,
    },
    Shutdown,
}

pub fn webview2_available() -> bool {
    if std::env::var_os("MICASPEAK_FORCE_WEBVIEW2_MISSING").is_some() {
        return false;
    }
    #[cfg(windows)]
    {
        for key in [
            "HKCU\\Software\\Microsoft\\EdgeUpdate\\Clients",
            "HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients",
        ] {
            let output = std::process::Command::new("reg")
                .args(["query", key, "/s", "/f", "pv"])
                .output();
            if output
                .map(|o| o.status.success() && !o.stdout.is_empty())
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        true
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
    disconnect_inner(&state).await;
    let identity = load_identity().map_err(|e| format!("身份文件无法读取：{e}"))?;
    let mut password = password.filter(|p| !p.is_empty());
    let mut auto_join = None;
    if let Some(id) = bookmark_id.as_deref() {
        let cfg = state.config.lock().await.clone();
        if let Some(bookmark) = cfg.bookmarks.iter().find(|b| b.id == id) {
            if password.is_none() {
                password = bookmark.password.clone().filter(|p| !p.is_empty());
            }
            auto_join = bookmark.last_channel.clone();
        }
    }
    debug!(?bookmark_id, ?auto_join, "connect: bookmark resolved");
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
    state.emit_snapshot(&app).await;
    let id = state.next_conn_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = mpsc::channel(8);
    *state.conn_tx.lock().await = Some(ConnOwner { id, tx });
    let shared = state.inner().clone();
    tauri::async_runtime::spawn(connection_loop(
        app,
        shared,
        rx,
        id,
        address,
        nickname,
        password,
        identity,
        auto_join,
    ));
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
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
        }
    }
}

/// 独占轮询事件流：转发状态/错误事件，并在首次连上后自动回连上次频道。
async fn driver_loop(
    app: AppHandle,
    state: AppState,
    mut sync: SyncConnection,
    address: String,
    cmd_tx: Option<mpsc::Sender<ConnCommand>>,
    mut auto_join: Option<String>,
    id: u64,
) {
    while let Some(item) = sync.next().await {
        // 连接已被替换/断开（owner 换人）后不再发布任何状态，
        // 否则会把 disconnect 重置好的界面覆盖成幽灵"已连接"。
        if !is_current(&state, id).await {
            break;
        }
        match item {
            Ok(SyncStreamItem::BookEvents(_)) => {
                publish_state(&app, &state, &mut sync, &address, id).await;
                // 首次 BookEvents 时频道列表可能尚未就绪：找不到目标就保留
                // auto_join 等下一次事件重试，找到（含密码频道）才消费掉。
                if let Some(name) = &auto_join {
                    let find = state
                        .channels
                        .lock()
                        .await
                        .iter()
                        .find(|c| c.name == *name)
                        .map(|c| (c.id, c.password));
                    debug!(name = %name, ?find, "driver: auto-join target");
                    match find {
                        Some((channel_id, false)) => {
                            auto_join = None;
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
                        }
                        None => {}
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
                state.audio.play_packet(from, packet);
            }
            Ok(SyncStreamItem::DisconnectedTemporarily(reason)) => {
                publish_temporary_disconnect(&app, &state, reason, address.clone()).await;
            }
            Ok(_) => {}
            Err(e) => {
                publish_error(&app, &state, friendly_error(&e), Some(address.clone())).await;
                break;
            }
        }
    }
    cleanup_if_current(&state, id).await;
    state.emit_snapshot(&app).await;
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
