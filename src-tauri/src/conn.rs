use crate::app_state::AppState;
use crate::persistence::load_identity;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tsclientlib::prelude::*;
use tsclientlib::sync::{SyncConnection, SyncConnectionHandle, SyncStreamItem};
use tsclientlib::{Connection, DisconnectOptions, Identity};

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
    SelectChannel { id: u64, password: Option<String> },
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

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    address: String,
    nickname: String,
    password: Option<String>,
) -> Result<(), String> {
    disconnect_inner(&state).await;
    let identity = load_identity().map_err(|e| format!("身份文件无法读取：{e}"))?;
    {
        let mut payload = state.connection.lock().await;
        payload.status = "connecting".into();
        payload.reason = None;
        payload.server_address = Some(address.clone());
    }
    let _ = app.emit("connection://state", state.connection.lock().await.clone());
    let (tx, rx) = mpsc::channel(8);
    *state.conn_tx.lock().await = Some(tx);
    let shared = state.inner().clone();
    tauri::async_runtime::spawn(connection_loop(
        app, shared, rx, address, nickname, password, identity,
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
    let cfg = state.config.lock().await.clone();
    let b = cfg
        .bookmarks
        .first()
        .ok_or_else(|| "没有可重连的书签".to_string())?
        .clone();
    connect(app, state, b.address, b.nickname, b.password).await
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
        .clone()
        .ok_or_else(|| "当前没有活动连接".to_string())?;
    tx.send(ConnCommand::SelectChannel {
        id: channel_id,
        password,
    })
    .await
    .map_err(|_| "连接任务已结束".to_string())
}

async fn disconnect_inner(state: &AppState) {
    if let Some(tx) = state.conn_tx.lock().await.take() {
        let _ = tx.send(ConnCommand::Shutdown).await;
    }
    *state.channels.lock().await = Vec::new();
    *state.connection.lock().await = ConnectionPayload::default();
}

async fn connection_loop(
    app: AppHandle,
    state: AppState,
    mut rx: mpsc::Receiver<ConnCommand>,
    address: String,
    nickname: String,
    password: Option<String>,
    identity: Identity,
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
            publish_error(&app, &state, format!("连接失败：{e}"), Some(address)).await;
            return;
        }
    };
    let mut sync: SyncConnection = connection.into();
    let mut handle = sync.get_handle();
    loop {
        tokio::select! {
            command = rx.recv() => match command {
                Some(ConnCommand::Shutdown) | None => { let _ = handle.disconnect(DisconnectOptions::new()).await; break; }
                Some(ConnCommand::SelectChannel { id, password }) => {
                    if let Err(e) = move_to_channel(&mut handle, id, password).await { publish_error(&app, &state, format!("进入频道失败：{e}"), None).await; }
                }
            },
            item = sync.next() => match item {
                Some(Ok(SyncStreamItem::BookEvents(_))) => publish_state(&app, &state, &mut handle, &address).await,
                Some(Ok(SyncStreamItem::DisconnectedTemporarily(reason))) => publish_error(&app, &state, format!("连接已断开：{reason:?}"), Some(address.clone())).await,
                Some(Err(e)) => { publish_error(&app, &state, format!("连接错误：{e}"), Some(address.clone())).await; break; }
                None => break,
                _ => {}
            }
        }
    }
    *state.conn_tx.lock().await = None;
    *state.connection.lock().await = ConnectionPayload::default();
    state.emit_snapshot(&app).await;
}

async fn publish_state(
    app: &AppHandle,
    state: &AppState,
    handle: &mut SyncConnectionHandle,
    address: &str,
) {
    let result = handle
        .with_connection(|connection| {
            connection.get_state().ok().map(|book| {
                (
                    book.server.name.clone(),
                    book.own_client,
                    book.channels
                        .values()
                        .map(|c| {
                            (
                                c.id.0,
                                c.parent.0,
                                c.name.clone(),
                                c.order.0,
                                c.has_password,
                            )
                        })
                        .collect::<Vec<_>>(),
                    book.clients
                        .values()
                        .map(|c| (c.id.0, c.name.clone(), c.channel.0))
                        .collect::<Vec<_>>(),
                )
            })
        })
        .await;
    match result.ok().flatten() {
        Some((server_name, own_client, channels, clients)) => {
            let client_nodes = clients;
            let nodes = channels
                .into_iter()
                .map(|(id, parent, name, order, password)| ChannelNode {
                    id,
                    parent_id: (parent != 0).then_some(parent),
                    name,
                    order,
                    password: password.unwrap_or(false),
                    clients: client_nodes
                        .iter()
                        .filter(|(_, _, channel)| *channel == id)
                        .map(|(cid, name, _)| ClientNode {
                            id: *cid as u64,
                            name: name.clone(),
                            channel_id: id,
                            is_self: *cid as u16 == own_client.0,
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
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
        None => {}
    }
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
        .map_err(|e| e.to_string())
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
