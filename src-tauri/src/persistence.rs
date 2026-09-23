use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};
use tauri::State;
use tsclientlib::Identity;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookmarkConfig {
    pub id: String,
    pub address: String,
    pub nickname: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub last_channel: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub bookmarks: Vec<BookmarkConfig>,
    #[serde(default)]
    pub last_channel: Option<String>,
    /// M3 语音设置；旧配置文件缺省时整体取默认（PTT + 左 Ctrl）。
    #[serde(default)]
    pub voice: VoiceConfig,
}

fn default_voice_mode() -> String {
    "ptt".into()
}

fn default_ptt_key_vk() -> u32 {
    // docs/01 F5：默认左 Ctrl（低级键盘钩子路径支持纯修饰键）。
    0xA2
}

fn default_vad_threshold() -> f32 {
    0.5
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// "ptt" | "vad"
    #[serde(default = "default_voice_mode")]
    pub mode: String,
    /// Windows 虚拟键码（docs/01 F5 默认左 Ctrl = 0xA2）。
    #[serde(default = "default_ptt_key_vk")]
    pub ptt_key_vk: u32,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,
    #[serde(default)]
    pub denoise: bool,
    /// None = 系统默认设备。
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default)]
    pub output_device: Option<String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            mode: default_voice_mode(),
            ptt_key_vk: default_ptt_key_vk(),
            vad_threshold: default_vad_threshold(),
            denoise: false,
            input_device: None,
            output_device: None,
        }
    }
}

/// 快照里的语音设置视图（含钩子运行状态，设置页直接渲染）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceSettingsView {
    pub mode: String,
    pub ptt_key_vk: u32,
    pub vad_threshold: f32,
    pub denoise: bool,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub hotkey_installed: bool,
}

impl VoiceConfig {
    pub fn view(&self, hotkey_installed: bool) -> VoiceSettingsView {
        VoiceSettingsView {
            mode: self.mode.clone(),
            ptt_key_vk: self.ptt_key_vk,
            vad_threshold: self.vad_threshold,
            denoise: self.denoise,
            input_device: self.input_device.clone(),
            output_device: self.output_device.clone(),
            hotkey_installed,
        }
    }
}

impl AppConfig {
    pub fn public_bookmarks(&self) -> Vec<crate::conn::Bookmark> {
        self.bookmarks
            .iter()
            .map(|b| crate::conn::Bookmark {
                id: b.id.clone(),
                address: b.address.clone(),
                nickname: b.nickname.clone(),
                password_saved: b.password.as_ref().is_some_and(|p| !p.is_empty()),
                last_channel: b.last_channel.clone(),
            })
            .collect()
    }
}

pub fn data_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let portable = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    if portable.join("portable.dat").exists() {
        return portable.join("Data");
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| portable.to_path_buf())
        .join("MicaSpeak")
}

fn atomic_write(path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// 返回配置与给用户的提示；提示非空表示原文件损坏，已备份并恢复默认。
pub fn load_config() -> io::Result<(AppConfig, Option<String>)> {
    let path = data_root().join("config.json");
    match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(config) => Ok((config, None)),
            Err(_) => {
                let _ = fs::rename(&path, path.with_extension("json.bak"));
                let config = AppConfig::default();
                let _ = atomic_write(&path, &serde_json::to_vec_pretty(&config).unwrap());
                Ok((
                    config,
                    Some("配置文件 config.json 已损坏，已自动备份为 config.json.bak 并恢复默认设置。".into()),
                ))
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let config = AppConfig::default();
            let _ = atomic_write(&path, &serde_json::to_vec_pretty(&config).unwrap());
            Ok((config, None))
        }
        Err(e) => Err(e),
    }
}

pub fn save_config(config: &AppConfig) -> io::Result<()> {
    atomic_write(
        &data_root().join("config.json"),
        &serde_json::to_vec_pretty(config).unwrap(),
    )
}

pub fn load_identity() -> io::Result<Identity> {
    let path = data_root().join("identity.json");
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let identity = Identity::create();
            atomic_write(&path, &serde_json::to_vec_pretty(&identity).unwrap())?;
            Ok(identity)
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn save_bookmark(
    app: tauri::AppHandle,
    state: State<'_, crate::app_state::AppState>,
    address: String,
    nickname: String,
    password: Option<String>,
) -> Result<(), String> {
    let mut config = state.config.lock().await;
    let id = format!("{}-{}", address, nickname).replace(['/', '\\', ':'], "_");
    if let Some(existing) = config.bookmarks.iter_mut().find(|b| b.id == id) {
        existing.address = address;
        existing.nickname = nickname;
        if password.as_ref().is_some_and(|p| !p.is_empty()) {
            existing.password = password;
        }
    } else {
        config.bookmarks.push(BookmarkConfig {
            id,
            address,
            nickname,
            password,
            last_channel: None,
        });
    }
    save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    drop(config);
    state.emit_snapshot(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_bookmark(
    app: tauri::AppHandle,
    state: State<'_, crate::app_state::AppState>,
    id: String,
) -> Result<(), String> {
    let mut config = state.config.lock().await;
    config.bookmarks.retain(|bookmark| bookmark.id != id);
    save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    drop(config);
    state.emit_snapshot(&app).await;
    Ok(())
}
