use crate::conn::{AppSnapshot, ConnectionPayload};
use crate::persistence::{load_config, AppConfig};
use crate::audio::AudioManager;
use crate::chat::{ChatStore, ChatUpdate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

/// 一条说话状态（进快照与 voice://talking 事件）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TalkerState {
    pub client_id: u64,
    pub name: String,
    pub is_self: bool,
    pub last_active_ms: u64,
}

#[derive(Clone)]
struct TalkerEntry {
    name: String,
    is_self: bool,
    last_active: Instant,
    last_active_ms: u64,
}

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
    /// M2 音频生命周期：采集/编码/发送开关。
    pub audio: AudioManager,
    /// M4 聊天事实状态：按目标键的日志、未读、当前频道。
    pub chat: ChatStore,
    /// 说话人状态：client_id -> 条目（标准互斥锁：会被音频事件任务同步访问）。
    talking: Arc<StdMutex<HashMap<u64, TalkerEntry>>>,
    /// 自己的 client id（publish_state 更新；0 = 未知）。
    own_client: Arc<AtomicU64>,
    /// 自己的昵称（connect 时记录，用于 TalkingState）。
    self_nickname: Arc<StdMutex<String>>,
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
            audio: AudioManager::new(),
            chat: ChatStore::new(),
            talking: Arc::new(StdMutex::new(HashMap::new())),
            own_client: Arc::new(AtomicU64::new(0)),
            self_nickname: Arc::new(StdMutex::new("我".to_string())),
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
        let voice = {
            let config = self.config.lock().await;
            config.voice.view(crate::hotkey::installed())
        };
        let talking = self.talking_list();
        let own_channel_id = self.chat.own_channel();
        let channel_names: HashMap<u64, String> = channels
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let client_names: HashMap<u64, String> = channels
            .iter()
            .flat_map(|c| c.clients.iter())
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let chat = self.chat.view(&channel_names, &client_names);
        AppSnapshot {
            connection,
            channels,
            bookmarks,
            last_channel,
            runtime_available: self.runtime_available,
            talking,
            voice,
            own_channel_id,
            chat,
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

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn set_own_client(&self, id: u64) {
        self.own_client.store(id, Ordering::Relaxed);
    }

    pub fn own_client(&self) -> u64 {
        self.own_client.load(Ordering::Relaxed)
    }

    pub fn set_self_nickname(&self, name: String) {
        *self.self_nickname.lock().unwrap() = name;
    }

    pub fn self_nickname(&self) -> String {
        self.self_nickname.lock().unwrap().clone()
    }

    pub fn is_self(&self, client_id: u64) -> bool {
        self.own_client() == client_id
    }

    /// 从频道树查昵称（连接任务事件线程调用）。
    pub async fn client_name(&self, client_id: u64) -> Option<String> {
        self.channels
            .lock()
            .await
            .iter()
            .flat_map(|c| c.clients.iter())
            .find(|c| c.id == client_id)
            .map(|c| c.name.clone())
    }

    /// 插入/刷新说话人；返回是否新出现（需要广播 talking=true）。
    pub fn talking_start(&self, client_id: u64, name: String, is_self: bool) -> bool {
        let mut talking = self.talking.lock().unwrap();
        let now_ms = Self::now_ms();
        let fresh = !talking.contains_key(&client_id);
        talking.insert(
            client_id,
            TalkerEntry { name, is_self, last_active: Instant::now(), last_active_ms: now_ms },
        );
        fresh
    }

    /// 收包刷新活跃时间。
    pub fn talking_touch(&self, client_id: u64) {
        if let Some(entry) = self.talking.lock().unwrap().get_mut(&client_id) {
            entry.last_active = Instant::now();
            entry.last_active_ms = Self::now_ms();
        }
    }

    /// 移除说话人；返回 (昵称, 是否自己) 以便广播。
    pub fn talking_stop(&self, client_id: u64) -> Option<(String, bool)> {
        self.talking
            .lock()
            .unwrap()
            .remove(&client_id)
            .map(|e| (e.name, e.is_self))
    }

    /// 兜底清扫：超过 timeout 无包的说话人视为停止（覆盖丢失的结束事件）。
    /// 自己的条目由发送开关管理，跳过。
    pub fn talking_sweep(&self, timeout: Duration) -> Vec<(u64, String, bool)> {
        let mut talking = self.talking.lock().unwrap();
        let expired: Vec<u64> = talking
            .iter()
            .filter(|(id, e)| !e.is_self && e.last_active.elapsed() > timeout)
            .map(|(id, _)| *id)
            .collect();
        expired
            .into_iter()
            .filter_map(|id| talking.remove(&id).map(|e| (id, e.name, e.is_self)))
            .collect()
    }

    /// 清空（断开时）。
    pub fn talking_clear(&self) {
        self.talking.lock().unwrap().clear();
    }

    pub fn talking_list(&self) -> Vec<TalkerState> {
        self.talking
            .lock()
            .unwrap()
            .iter()
            .map(|(id, e)| TalkerState {
                client_id: *id,
                name: e.name.clone(),
                is_self: e.is_self,
                last_active_ms: e.last_active_ms,
            })
            .collect()
    }

    /// 从当前配置构造设备参数（设置命令在释放 config 锁后调用）。
    /// 模式/阈值/降噪走原子量即时生效，不经此路径。
    pub async fn audio_devices_from_config(&self) -> crate::audio::VoiceParams {
        let config = self.config.lock().await;
        let v = &config.voice;
        crate::audio::VoiceParams {
            input_device: v.input_device.clone(),
            output_device: v.output_device.clone(),
        }
    }

    /// 广播 voice://talking。
    pub async fn emit_talking(&self, app: &AppHandle, client_id: u64, name: String, talking: bool) {
        let _ = app.emit(
            "voice://talking",
            serde_json::json!({
                "client_id": client_id,
                "name": name,
                "talking": talking,
                "is_self": self.is_self(client_id),
                "timestamp": Self::now_ms(),
            }),
        );
    }

    /// 广播 chat://message（含 Rust 权威未读数）。
    pub fn emit_chat_update(&self, app: &AppHandle, update: &ChatUpdate) {
        let _ = app.emit(
            "chat://message",
            serde_json::json!({
                "kind": update.key.kind(),
                "target_id": update.key.target_id(),
                "unread": update.unread,
                "message": update.message,
            }),
        );
    }
}
