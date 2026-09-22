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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub bookmarks: Vec<BookmarkConfig>,
    #[serde(default)]
    pub last_channel: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bookmarks: Vec::new(),
            last_channel: None,
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

pub fn load_config() -> io::Result<AppConfig> {
    let path = data_root().join("config.json");
    match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(config) => Ok(config),
            Err(_) => {
                let _ = fs::rename(&path, path.with_extension("json.bak"));
                let config = AppConfig::default();
                let _ = atomic_write(&path, &serde_json::to_vec_pretty(&config).unwrap());
                Ok(config)
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let config = AppConfig::default();
            let _ = atomic_write(&path, &serde_json::to_vec_pretty(&config).unwrap());
            Ok(config)
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
    save_config(&config).map_err(|e| format!("保存配置失败：{e}"))
}

#[tauri::command]
pub async fn delete_bookmark(
    state: State<'_, crate::app_state::AppState>,
    id: String,
) -> Result<(), String> {
    let mut config = state.config.lock().await;
    config.bookmarks.retain(|bookmark| bookmark.id != id);
    save_config(&config).map_err(|e| format!("保存配置失败：{e}"))
}
