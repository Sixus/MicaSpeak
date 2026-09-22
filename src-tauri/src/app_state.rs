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
    // WebView2 安装状态进程内不变；只在外层 main 窗口创建前检测一次，
    // 避免在异步命令里反复同步 spawn reg（曾观察到偶发挂死）。
    runtime_available: bool,
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
            runtime_available: crate::conn::webview2_available(),
            pending_error: Arc::new(Mutex::new(notice)),
        }
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        // 注意：不能在同一个结构体字面量里对同一 tokio Mutex 加锁两次——
        // 临时 MutexGuard 活到整条 let 语句结束，第二次 lock() 会自己等自己死锁。
        let connection = self.connection.lock().await.clone();
        let channels = self.channels.lock().await.clone();
        let (bookmarks, last_channel) = {
            let config = self.config.lock().await;
            (config.public_bookmarks(), config.last_channel.clone())
        };
        AppSnapshot {
            connection,
            channels,
            bookmarks,
            last_channel,
            runtime_available: self.runtime_available,
        }
    }

    pub async fn emit_snapshot(&self, app: &AppHandle) {
        let _ = app.emit("app://snapshot", self.snapshot().await);
    }

    /// 启动期产生的待提示（如 config 损坏）。在 Webview 尚未就绪时 emit 会丢，
    /// 所以由首次 get_app_snapshot 取走并补发。
    pub async fn take_pending_error(&self) -> Option<String> {
        self.pending_error.lock().await.take()
    }

    pub async fn emit_initial(&self, app: &AppHandle) {
        if !self.runtime_available {
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
