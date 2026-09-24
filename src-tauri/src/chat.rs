use crate::app_state::AppState;
use crate::conn::ConnCommand;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tokio::sync::oneshot;

/// 聊天标签键：频道消息按频道 id、私聊按对方 client id 路由（任务卡 A3）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatKey {
    Channel(u64),
    Client(u64),
}

impl ChatKey {
    pub fn kind(&self) -> &'static str {
        match self {
            ChatKey::Channel(_) => "channel",
            ChatKey::Client(_) => "private",
        }
    }
    pub fn target_id(&self) -> u64 {
        match self {
            ChatKey::Channel(id) | ChatKey::Client(id) => *id,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: u64,
    pub from_client_id: u64,
    pub from_name: String,
    pub is_self: bool,
    pub text: String,
    pub time_ms: u64,
}

/// 单个标签的日志：消息上限 500 条（任务卡 A1），未读由 Rust 权威维护。
struct ChatLog {
    title: String,
    messages: VecDeque<ChatMessage>,
    unread: u32,
}

const MAX_MESSAGES: usize = 500;

impl ChatLog {
    fn new(title: String) -> Self {
        Self { title, messages: VecDeque::new(), unread: 0 }
    }

    fn push(&mut self, message: ChatMessage) {
        if self.messages.len() >= MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatTabView {
    pub kind: String,
    pub target_id: u64,
    pub title: String,
    pub unread: u32,
    pub messages: Vec<ChatMessage>,
}

/// 一次落账的结果，调用方据此发 chat://message。
pub struct ChatUpdate {
    pub key: ChatKey,
    pub unread: u32,
    pub message: ChatMessage,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 聊天事实状态：运行内保留（F3：重启不落盘）。
#[derive(Clone)]
pub struct ChatStore {
    logs: Arc<StdMutex<HashMap<ChatKey, ChatLog>>>,
    next_message_id: Arc<AtomicU64>,
    /// 自己当前所在频道（0=未知）；频道消息归属与未读判定的事实依据。
    own_channel: Arc<AtomicU64>,
}

impl ChatStore {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(StdMutex::new(HashMap::new())),
            next_message_id: Arc::new(AtomicU64::new(1)),
            own_channel: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn own_channel(&self) -> u64 {
        self.own_channel.load(Ordering::Relaxed)
    }

    pub fn set_own_channel(&self, id: u64) {
        self.own_channel.store(id, Ordering::Relaxed);
    }

    /// 收到他人消息。频道消息归属当前频道；不在当前频道的标签计未读。
    /// 私聊（M4b 接 UI）在无标签 UI 阶段一律计未读。
    pub fn receive(&self, key: ChatKey, title: &str, from_client_id: u64, from_name: String, text: String) -> ChatUpdate {
        let message = ChatMessage {
            id: self.next_id(),
            from_client_id,
            from_name,
            is_self: false,
            text,
            time_ms: now_ms(),
        };
        let unread = {
            let mut logs = self.logs.lock().unwrap();
            let log = logs.entry(key).or_insert_with(|| ChatLog::new(title.to_string()));
            if log.title.is_empty() {
                log.title = title.to_string();
            }
            log.push(message.clone());
            let viewed = matches!(key, ChatKey::Channel(id) if id == self.own_channel());
            if !viewed {
                log.unread = log.unread.saturating_add(1);
            }
            log.unread
        };
        ChatUpdate { key, unread, message }
    }

    /// 自己发送成功后落账（服务器已回执，不算未读）。
    pub fn record_self(&self, key: ChatKey, title: &str, own_client_id: u64, from_name: String, text: String) -> ChatUpdate {
        let message = ChatMessage {
            id: self.next_id(),
            from_client_id: own_client_id,
            from_name,
            is_self: true,
            text,
            time_ms: now_ms(),
        };
        let unread = {
            let mut logs = self.logs.lock().unwrap();
            let log = logs.entry(key).or_insert_with(|| ChatLog::new(title.to_string()));
            if log.title.is_empty() {
                log.title = title.to_string();
            }
            log.push(message.clone());
            log.unread
        };
        ChatUpdate { key, unread, message }
    }

    /// 快照视图：标签按 当前频道 > 其他频道 > 私聊 排序；
    /// 标题优先用频道树现名，查不到回退最后已知标题。
    pub fn view(&self, channel_names: &HashMap<u64, String>) -> Vec<ChatTabView> {
        let mut logs = self.logs.lock().unwrap();
        let own_channel = self.own_channel();
        let mut tabs: Vec<ChatTabView> = logs
            .iter_mut()
            .map(|(key, log)| {
                let target_id = key.target_id();
                let title = channel_names
                    .get(&target_id)
                    .cloned()
                    .unwrap_or_else(|| log.title.clone());
                if !title.is_empty() {
                    log.title = title.clone();
                }
                ChatTabView {
                    kind: key.kind().to_string(),
                    target_id,
                    title,
                    unread: log.unread,
                    messages: log.messages.iter().cloned().collect(),
                }
            })
            .collect();
        tabs.sort_by_key(|tab| {
            let is_channel = tab.kind == "channel";
            (
                std::cmp::Reverse(is_channel && tab.target_id == own_channel),
                std::cmp::Reverse(is_channel),
                tab.target_id,
            )
        });
        tabs
    }

    fn next_id(&self) -> u64 {
        self.next_message_id.fetch_add(1, Ordering::Relaxed)
    }
}

// ---- open_url：严格校验后交系统默认浏览器，零新依赖 ----

/// 只放行 http/https 绝对 URL；拒绝引号、控制字符、反斜杠等，
/// 防 URL 文本经命令行解释产生注入面。
pub fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() || url.len() > 2048 {
        return Err("链接无效或过长".into());
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("只支持 http/https 链接".into());
    }
    if url
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '\'' | '\\' | '<' | '>' | '`'))
    {
        return Err("链接包含非法字符".into());
    }
    Ok(url.to_string())
}

#[cfg(windows)]
pub fn open_in_browser(url: &str) -> Result<(), String> {
    let url = validate_url(url)?;
    // explorer 把整条 URL 作为单个参数处理，不经 shell 解析
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开浏览器：{e}"))
}

#[cfg(not(windows))]
pub fn open_in_browser(url: &str) -> Result<(), String> {
    validate_url(url).map(|_| ())
}

// ---- Tauri commands ----

/// 频道消息：经连接任务串行发送，服务器回执即成功（任务卡 A2）。
#[tauri::command]
pub async fn send_channel_message(state: State<'_, AppState>, text: String) -> Result<(), String> {
    let text = text.trim_end().to_string();
    if text.is_empty() {
        return Err("消息不能为空".into());
    }
    if text.len() > 2000 {
        return Err("消息过长（上限 2000 字符）".into());
    }
    let tx = state
        .conn_tx
        .lock()
        .await
        .as_ref()
        .map(|o| o.tx.clone())
        .ok_or_else(|| "当前没有活动连接".to_string())?;
    let (result_tx, result_rx) = oneshot::channel();
    tx.send(ConnCommand::SendChannelMessage { text, result: result_tx })
        .await
        .map_err(|_| "连接任务已结束".to_string())?;
    result_rx.await.map_err(|_| "连接任务已结束".to_string())?
}

/// 在系统默认浏览器打开聊天里的链接。
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open_in_browser(&url)
}
