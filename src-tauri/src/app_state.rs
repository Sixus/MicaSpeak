use crate::conn::{AppSnapshot, ConnectionPayload};
use crate::persistence::{load_config, AppConfig};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<AppConfig>>,
    pub connection: Arc<Mutex<ConnectionPayload>>,
    pub channels: Arc<Mutex<Vec<crate::conn::ChannelNode>>>,
    pub conn_tx: Arc<Mutex<Option<mpsc::Sender<crate::conn::ConnCommand>>>>,
}

impl AppState {
    pub fn new() -> Self {
        let config = load_config().unwrap_or_default();
        Self {
            config: Arc::new(Mutex::new(config)),
            connection: Arc::new(Mutex::new(ConnectionPayload::default())),
            channels: Arc::new(Mutex::new(Vec::new())),
            conn_tx: Arc::new(Mutex::new(None)),
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
        self.emit_snapshot(app).await;
    }
}
