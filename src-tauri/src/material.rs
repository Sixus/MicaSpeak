//! M5 A：窗口材质（docs/09 阶段 A）。Win11（build ≥ 22000）对主窗口尝试
//! apply_mica（window-vibrancy，docs/02 §2 选型）；失败、Win10 或
//! `--force-fallback` 时使用实体背景（React 用不透明背景，窗口透明度由
//! CSS 承担，视觉与普通窗口无差异）。
//!
//! 深浅主题跟随系统：tauri 的 ThemeChanged 事件到达时按当前深浅重新着色。

use serde::Serialize;
use std::fmt;
use std::sync::Mutex as StdMutex;
use tauri::{Manager, WebviewWindow};
use tracing::info;

/// 快照里的材质事实：material = "mica" | "solid"。
#[derive(Clone, Debug, Serialize)]
pub struct MaterialState {
    pub material: String,
    /// 回退原因（mica 成功时为 None）。
    pub reason: Option<String>,
    pub force_fallback: bool,
}

impl Default for MaterialState {
    fn default() -> Self {
        Self { material: "solid".into(), reason: None, force_fallback: false }
    }
}

impl fmt::Display for MaterialState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.material == "mica" {
            write!(f, "Mica")
        } else if self.force_fallback {
            write!(f, "实体（--force-fallback）")
        } else if let Some(reason) = &self.reason {
            write!(f, "实体（回退：{reason}）")
        } else {
            write!(f, "实体")
        }
    }
}

/// 材质决策进程内不变（决定一次）；深浅色重着色只改 DWM 属性不改决策。
#[derive(Clone, Default)]
pub struct MaterialManager {
    state: std::sync::Arc<StdMutex<MaterialState>>,
}

impl MaterialManager {
    pub fn get(&self) -> MaterialState {
        self.state.lock().unwrap().clone()
    }
    fn set(&self, state: MaterialState) {
        *self.state.lock().unwrap() = state;
    }
    fn is_mica(&self) -> bool {
        self.state.lock().unwrap().material == "mica"
    }
}

/// `--force-fallback`：任务卡验收第 2 项的入口。
pub fn force_fallback_requested() -> bool {
    std::env::args().any(|a| a == "--force-fallback")
}

/// Windows 版本号（build），查询失败返回 None（视作不支持 Mica）。
#[cfg(windows)]
pub fn windows_build() -> Option<u32> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(idx) = line.find("CurrentBuildNumber") {
            let rest = &line[idx + "CurrentBuildNumber".len()..];
            let build: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = build.parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn windows_build() -> Option<u32> {
    None
}

/// 当前系统深浅（tauri 读取；读不到按浅色）。
fn window_is_dark(window: &WebviewWindow) -> bool {
    window.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(false)
}

/// 启动时对主窗口应用材质。幂等（重复调用安全：以首次决策为准更新事实）。
pub fn apply_main_window_material(manager: &MaterialManager, window: &WebviewWindow) {
    let mut state = MaterialState { force_fallback: force_fallback_requested(), ..Default::default() };
    let build = windows_build();
    let dark = window_is_dark(window);
    #[cfg(windows)]
    if !state.force_fallback && build.is_some_and(|b| b >= 22_000) {
        match window_vibrancy::apply_mica(window, Some(dark)) {
            Ok(()) => {
                state.material = "mica".into();
                state.reason = None;
            }
            Err(e) => {
                state.reason = Some(format!("apply_mica 失败：{e}"));
            }
        }
    }
    #[cfg(not(windows))]
    if !state.force_fallback {
        state.reason = Some("非 Windows 平台".into());
    }
    if state.material != "mica" && state.reason.is_none() && !state.force_fallback {
        state.reason = Some(match build {
            Some(b) => format!("Windows build {b} < 22000"),
            None => "无法读取 Windows 版本".into(),
        });
    }
    info!(material = %state.material, build = ?build, force = state.force_fallback, "窗口材质已应用");
    manager.set(state);
}

/// 系统深浅切换（ThemeChanged）：Mica 生效时按新主题重新着色。
/// 返回 true 表示重新应用了材质（调用方可决定是否刷新快照）。
pub fn on_theme_changed(manager: &MaterialManager, window: &WebviewWindow) -> bool {
    if !manager.is_mica() {
        return false;
    }
    let dark = window_is_dark(window);
    #[cfg(windows)]
    {
        let _ = window_vibrancy::apply_mica(window, Some(dark));
    }
    info!(dark, "系统深浅切换，Mica 已重新着色");
    true
}

/// 从应用句柄取主窗口并应用材质（setup 里调用）；
/// 同时注册系统深浅切换跟随（ThemeChanged → Mica 重新着色）。
pub fn init_main_window_material(
    manager: &MaterialManager,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    apply_main_window_material(manager, &window);
    let manager = manager.clone();
    let theme_window = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::ThemeChanged(_)) {
            on_theme_changed(&manager, &theme_window);
        }
    });
    Ok(())
}
