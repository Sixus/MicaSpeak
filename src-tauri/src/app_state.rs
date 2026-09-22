use crate::conn::{AppSnapshot, ConnectionPayload};
use crate::persistence::{load_config, AppConfig};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

/// 当前连接使用的入口信息；密码只保存在内存，不进入日志、事件或前端。
#[derive(Clone)]
pub struct ActiveConnection {
    pub address: String,
    pub nickname: String,
    pub password: Option<String>,
    pub bookmark_id: Option<String>,
}

#[derive(Clone)]
pub struct ConnOwner {
    pub id: u64,
    pub tx: mpsc::Sender<crate::conn::ConnCommand>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<AppConfig>>,
    pub connection: Arc<Mutex<ConnectionPayload>>,
    pub channels: Arc<Mutex<Vec<crate::conn::ChannelNode>>>,
    pub conn_tx: Arc<Mutex<Option<ConnOwner>>>,
    pub active: Arc<Mutex<Option<ActiveConnection>>>,
    pub next_conn_id: Arc<AtomicU64>,
    pending_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        let (config, notice) = load_config()
            .unwrap_or_else(|e| (AppConfig::default(), Some(format!("配置文件读取失败：{e}"))));
        Self {
            config: Arc::new(Mutex::new(config)),
            connection: Arc::new(Mutex::new(ConnectionPayload::default())),
            channels: Arc::new(Mutex::new(Vec::new())),
            conn_tx: Arc::new(Mutex::new(None)),
            active: Arc::new(Mutex::new(None)),
            next_conn_id: Arc::new(AtomicU64::new(0)),
            pending_error: Arc::new(Mutex::new(notice)),
        }
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            connection: self.connection.lock().await.clone(),
            channels: self.channels.lock().await.clone(),
            bookmarks: self.config.lock().await.public_bookmarks(),
            last_channel: self.config.lock().await.last_channel.clone(),
            runtime_available: crate::conn::webview2_available(),
        }
    }

    pub async fn emit_snapshot(&self, app: &AppHandle) {
        let _ = app.emit("app://snapshot", self.snapshot().await);
    }

    pub async fn emit_initial(&self, app: &AppHandle) {
        if !crate::conn::webview2_available() {
            let _ = app.emit(
                "runtime://webview2-missing",
                serde_json::json!({
                    "message": "需要安装 Microsoft Edge WebView2 Runtime"
                }),
            );
        }
        if let Some(message) = self.pending_error.lock().await.take() {
            let _ = app.emit("error://user", serde_json::json!({ "message": message }));
        }
        self.emit_snapshot(app).await;
    }
}
