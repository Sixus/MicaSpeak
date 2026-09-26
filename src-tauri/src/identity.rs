//! M6a 身份存储与管理。
//! 硬约束（docs/02 §4/§5）：私钥只存在于 Rust 内存与 Data/identities/ 文件；
//! 不进日志、Tauri 事件、前端 store 或截图。前端只拿脱敏视图 IdentityView。
//!
//! 解析官方客户端"无密码"导出串（格式 `计数器V密钥`）：tsproto 的
//! Identity::new_from_ts_str 按 'V' 切分后走 EccKeyPrivP256::import_str
//! （tsproto-types crypto.rs:305，含 to_ts_obfuscated 反混淆路径）。
//! 带密码导出串是加密格式，解析失败为预期（任务卡已知坑）。

use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, State};
use tsclientlib::Identity;

use tracing::info;

use crate::app_state::AppState;
use crate::persistence::{data_root, AppConfig, save_config};

/// 每个身份一个文件：Data/identities/<id>.json。
/// identity 字段直接复用 tsclientlib::Identity 的 serde（key/counter/max_counter），
/// 与旧 identity.json 同构，旧文件可无损迁移。
#[derive(Clone, Serialize, Deserialize)]
pub struct IdentityRecord {
    pub id: String,
    pub label: String,
    /// base64(sha1(公钥))，官方 Unique ID；文件内保存完整值，视图里脱敏。
    pub uid: String,
    pub created_ms: u64,
    pub identity: Identity,
}

/// 快照/列表视图：不含任何密钥材料，uid 脱敏。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityView {
    pub id: String,
    pub label: String,
    pub uid_masked: String,
    pub level: u8,
    pub active: bool,
}

fn identities_dir() -> io::Result<PathBuf> {
    let dir = data_root().join("identities");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 脱敏：保留前 6 后 4（uid 为 28 字符 base64）。
pub fn mask_uid(uid: &str) -> String {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() <= 12 {
        return uid.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

fn view(record: &IdentityRecord, active_id: Option<&str>) -> IdentityView {
    IdentityView {
        id: record.id.clone(),
        label: record.label.clone(),
        uid_masked: mask_uid(&record.uid),
        level: record.identity.level(),
        active: active_id == Some(record.id.as_str()),
    }
}

/// 文件名安全的 id：uid 去掉 base64 中的 / 和 +。
fn id_from_uid(uid: &str) -> String {
    uid.replace(['/', '+', '\\', '='], "-")
}

fn write_record(record: &IdentityRecord) -> io::Result<()> {
    let dir = identities_dir()?;
    crate::persistence::atomic_write_pub(&dir.join(format!("{}.json", record.id)),
        &serde_json::to_vec_pretty(record).expect("IdentityRecord 序列化不会失败"))
}

fn read_record(path: &std::path::Path) -> io::Result<IdentityRecord> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{} 解析失败：{e}", path.display())))
}

/// 列出全部身份（按创建时间）。目录缺失/读取失败返回空（调用方兜底建默认）。
pub fn list_records() -> Vec<IdentityRecord> {
    let Ok(dir) = identities_dir() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
    let mut records: Vec<IdentityRecord> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| read_record(&e.path()).ok())
        .collect();
    records.sort_by_key(|r| r.created_ms);
    records
}

/// 快照视图（AppState::snapshot 调用，同步、无锁）。
pub fn list_views(config: &AppConfig) -> Vec<IdentityView> {
    let active = config.active_identity.as_deref();
    list_records().iter().map(|r| view(r, active)).collect()
}

/// 启动初始化：迁移旧 identity.json + 保证至少存在一个身份且 active 指向有效项。
/// 返回给用户的提示（写进 pending_error，成功迁移也提示备份位置）。
pub fn init_storage(config: &mut AppConfig) -> Option<String> {
    match init_storage_inner(config) {
        Ok(notice) => notice,
        Err(e) => Some(format!("身份存储初始化失败：{e}")),
    }
}

fn init_storage_inner(config: &mut AppConfig) -> io::Result<Option<String>> {
    let mut notice: Option<String> = None;
    let legacy = data_root().join("identity.json");
    let mut records = list_records();
    if records.is_empty() && legacy.exists() {
        // 旧单身份文件迁移：原样转存为 identities/<id>.json，原文件改名 .bak 保留。
        let bytes = fs::read(&legacy)?;
        let identity: Identity = serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("旧 identity.json 解析失败：{e}")))?;
        let uid = identity.key().to_pub().get_uid();
        let record = IdentityRecord {
            id: id_from_uid(&uid),
            label: "我的身份".into(),
            uid,
            created_ms: now_ms(),
            identity,
        };
        write_record(&record)?;
        fs::rename(&legacy, legacy.with_extension("json.bak"))?;
        info_migrated(&record.id);
        notice = Some("已把旧身份文件迁移到 Data\\identities\\，原文件保留为 identity.json.bak。".into());
        records = list_records();
    }
    if records.is_empty() {
        let record = new_record("默认身份".into())?;
        write_record(&record)?;
        config.active_identity = Some(record.id.clone());
        return Ok(notice);
    }
    let valid = records.iter().any(|r| Some(&r.id) == config.active_identity.as_ref());
    if !valid {
        config.active_identity = Some(records[0].id.clone());
    }
    Ok(notice)
}

fn info_migrated(id: &str) {
    info!(id, "旧 identity.json 已迁移到 Data/identities/，原文件保留为 .bak");
}

fn new_record(label: String) -> io::Result<IdentityRecord> {
    let identity = Identity::create();
    let uid = identity.key().to_pub().get_uid();
    Ok(IdentityRecord {
        id: id_from_uid(&uid),
        label,
        uid,
        created_ms: now_ms(),
        identity,
    })
}

/// 连接时取当前身份（conn::connect_inner 调用）。
pub async fn load_active_identity(
    config: &tokio::sync::Mutex<AppConfig>,
) -> Result<IdentityRecord, String> {
    let active_id = config.lock().await.active_identity.clone();
    let records = list_records();
    let target = active_id
        .as_deref()
        .and_then(|id| records.iter().find(|r| r.id == id))
        .or_else(|| records.first());
    let Some(record) = target else {
        return Err("没有可用身份：请到 设置 → 身份 新建或导入一个身份".into());
    };
    Ok(record.clone())
}

/// 写回（升级计数器等场景）。
pub fn save_record(record: &IdentityRecord) -> io::Result<()> {
    write_record(record)
}

// ---- Tauri commands ----

/// 导入官方客户端"无密码"导出的身份字符串。成功返回新身份视图。
#[tauri::command]
pub async fn import_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    data: String,
    label: Option<String>,
) -> Result<IdentityView, String> {
    let text = data.trim().to_string();
    if text.is_empty() {
        return Err("请先粘贴身份字符串".into());
    }
    let identity = parse_official_identity(&text)?;
    let uid = identity.key().to_pub().get_uid();
    let id = id_from_uid(&uid);
    let existing = list_records();
    if existing.iter().any(|r| r.id == id) {
        return Err("该身份已经存在（Unique ID 相同），无需重复导入".into());
    }
    let label = match label {
        Some(l) if !l.trim().is_empty() => l.trim().to_string(),
        _ => format!("导入的身份 {}", existing.len() + 1),
    };
    let record = IdentityRecord { id: id.clone(), label, uid, created_ms: now_ms(), identity };
    write_record(&record).map_err(|e| format!("保存身份文件失败：{e}"))?;
    {
        let mut config = state.config.lock().await;
        // 首个身份自动设为当前。
        if config.active_identity.is_none() {
            config.active_identity = Some(id.clone());
        }
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    info!(id = %record.id, "身份导入成功");
    state.emit_snapshot(&app).await;
    Ok(view(&record, state.config.lock().await.active_identity.as_deref()))
}

/// 区分三类失败：空输入已在上面拦；官方带密码导出/乱码 → 中文提示。
fn parse_official_identity(text: &str) -> Result<Identity, String> {
    // 官方串形如 "123456V<base64>"；先按 V 串格式试，再退回裸密钥格式。
    if let Ok(identity) = Identity::new_from_ts_str(text) {
        return Ok(identity);
    }
    if let Ok(identity) = Identity::new_from_str(text) {
        return Ok(identity);
    }
    let looks_official = text.split('V').next().is_some_and(|head| !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()));
    if looks_official {
        Err("身份字符串无法解析。官方客户端“带密码”导出的身份不受支持，请在官方客户端中导出时把密码留空后重试".into())
    } else {
        Err("身份字符串无法解析：内容不是有效的 TeamSpeak 身份格式".into())
    }
}

/// 新建一个随机身份（等级 8，与官方客户端新建默认一致）。
#[tauri::command]
pub async fn create_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    label: Option<String>,
) -> Result<IdentityView, String> {
    let count = list_records().len();
    let label = match label {
        Some(l) if !l.trim().is_empty() => l.trim().to_string(),
        _ => format!("身份 {}", count + 1),
    };
    let record = new_record(label).map_err(|e| format!("创建身份失败：{e}"))?;
    write_record(&record).map_err(|e| format!("保存身份文件失败：{e}"))?;
    let mut config = state.config.lock().await;
    if config.active_identity.is_none() {
        config.active_identity = Some(record.id.clone());
    }
    save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    info!(id = %record.id, "新建身份成功");
    drop(config);
    state.emit_snapshot(&app).await;
    Ok(view(&record, state.config.lock().await.active_identity.as_deref()))
}

/// 删除身份（前端负责二次确认）。删除当前身份时自动指向剩余第一个。
#[tauri::command]
pub async fn delete_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut records = list_records();
    let Some(pos) = records.iter().position(|r| r.id == id) else {
        return Err("身份不存在或已删除".into());
    };
    if records.len() == 1 {
        return Err("至少保留一个身份，不能删除最后一个".into());
    }
    records.remove(pos);
    let path = identities_dir().map_err(|e| e.to_string())?.join(format!("{id}.json"));
    let backup = path.with_extension("json.deleted");
    // 不直接销毁：改名 .deleted 留一次反悔机会，下次启动或重建时不清理（占用极小）。
    fs::rename(&path, &backup).map_err(|e| format!("删除身份文件失败：{e}"))?;
    {
        let mut config = state.config.lock().await;
        if config.active_identity.as_deref() == Some(id.as_str()) {
            config.active_identity = records.first().map(|r| r.id.clone());
        }
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    info!(id, "身份已删除（文件改名 .deleted 保留）");
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 切换当前身份。连接中切换只改指针：需断开重连才生效（任务卡 A3 提示需重连）。
#[tauri::command]
pub async fn set_active_identity(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let records = list_records();
    let Some(record) = records.iter().find(|r| r.id == id) else {
        return Err("身份不存在或已删除".into());
    };
    {
        let mut config = state.config.lock().await;
        config.active_identity = Some(id.clone());
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    info!(id, "已切换当前身份");
    state.emit_snapshot(&app).await;
    let connected = state.connection.lock().await.status == "connected"
        || state.connection.lock().await.status == "connecting";
    let _ = record;
    if connected {
        Ok("身份已切换。当前连接仍在使用旧身份，断开并重新连接后生效".into())
    } else {
        Ok("身份已切换，下次连接生效".into())
    }
}

/// 导出为官方"无密码"格式字符串（计数器V密钥）。
/// 私钥警告确认在前端完成；返回值含私钥，禁止写入日志。
#[tauri::command]
pub async fn export_identity(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let records = list_records();
    let Some(record) = records.iter().find(|r| r.id == id) else {
        return Err("身份不存在或已删除".into());
    };
    let _ = state;
    Ok(format!("{}V{}", record.identity.counter(), record.identity.key().to_ts_obfuscated()))
}
