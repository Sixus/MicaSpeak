//! 窗口材质：Win11 22H2（build ≥ 22621）对主窗口应用系统 Acrylic 背衬
//! （DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW，经 window-vibrancy
//! apply_acrylic 下发；docs/02 选型 Mica，实机验收显色过淡，经用户决定换
//! Acrylic）。22H2 以下、失败或 `--force-fallback` 时使用实体背景
//! （React 用不透明背景，窗口透明度由 CSS 承担，视觉与普通窗口无差异）。
//!
//! 深浅主题跟随系统：tauri 的 ThemeChanged 事件到达时按当前深浅重新着色。

use serde::Serialize;
use std::fmt;
use std::sync::Mutex as StdMutex;
use tauri::{Manager, WebviewWindow};
use tracing::info;

/// 快照里的材质事实：material = "acrylic" | "solid"。
#[derive(Clone, Debug, Serialize)]
pub struct MaterialState {
    pub material: String,
    /// 回退原因（acrylic 成功时为 None）。
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
        if self.material == "acrylic" {
            write!(f, "Acrylic")
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
    fn is_acrylic(&self) -> bool {
        self.state.lock().unwrap().material == "acrylic"
    }
}

/// `--force-fallback`：任务卡验收第 2 项的入口。
pub fn force_fallback_requested() -> bool {
    std::env::args().any(|a| a == "--force-fallback")
}

/// Windows 版本号（build），注册表直读；读取失败返回 None（视作不支持 Acrylic）。
#[cfg(windows)]
pub fn windows_build() -> Option<u32> {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let v = crate::registry::read_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )?;
    v.trim().parse::<u32>().ok()
}

#[cfg(not(windows))]
pub fn windows_build() -> Option<u32> {
    None
}

/// 当前系统深浅（tauri 读取；读不到按浅色）。
fn window_is_dark(window: &WebviewWindow) -> bool {
    window.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(false)
}

/// 应用系统 Acrylic：immersive dark mode 由本模块直设（apply_acrylic 不代设，
/// 缺了它暗色系统下材质仍是浅色），背衬经 window-vibrancy 下发——build ≥ 22523
/// 走 DWMSBT_TRANSIENTWINDOW 系统背衬（拖动流畅），更老系统会走 AccentPolicy
/// （拖动卡顿），由调用方的 22621 闸门排除。
#[cfg(windows)]
fn apply_acrylic_backdrop(window: &WebviewWindow, dark: bool) -> Result<(), String> {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let value: i32 = dark as i32;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
        .map_err(|e| format!("immersive dark mode 设置失败：{e}"))?;
    }
    window_vibrancy::apply_acrylic(window, None).map_err(|e| e.to_string())
}

/// 启动时对主窗口应用材质。幂等（重复调用安全：以首次决策为准更新事实）。
pub fn apply_main_window_material(manager: &MaterialManager, window: &WebviewWindow) {
    let mut state = MaterialState { force_fallback: force_fallback_requested(), ..Default::default() };
    let build = windows_build();
    let dark = window_is_dark(window);
    #[cfg(windows)]
    if !state.force_fallback && build.is_some_and(|b| b >= 22_621) {
        match apply_acrylic_backdrop(window, dark) {
            Ok(()) => {
                state.material = "acrylic".into();
                state.reason = None;
            }
            Err(e) => {
                state.reason = Some(format!("apply_acrylic 失败：{e}"));
            }
        }
    }
    #[cfg(not(windows))]
    if !state.force_fallback {
        state.reason = Some("非 Windows 平台".into());
    }
    if state.material != "mica" && state.reason.is_none() && !state.force_fallback {
        state.reason = Some(match build {
            Some(b) => format!("Windows build {b} < 22621（Acrylic 需 Win11 22H2）"),
            None => "无法读取 Windows 版本".into(),
        });
    }
    info!(material = %state.material, build = ?build, force = state.force_fallback, "窗口材质已应用");
    manager.set(state);
}

/// 系统深浅切换（ThemeChanged）：Mica 生效时按新主题重新着色。
/// 返回 true 表示重新应用了材质（调用方可决定是否刷新快照）。
pub fn on_theme_changed(manager: &MaterialManager, window: &WebviewWindow) -> bool {
    if !manager.is_acrylic() {
        return false;
    }
    let dark = window_is_dark(window);
    #[cfg(windows)]
    {
        let _ = apply_acrylic_backdrop(window, dark);
    }
    info!(dark, "系统深浅切换，Acrylic 已重新着色");
    true
}

/// 从应用句柄取主窗口并应用材质（setup 里调用）；
/// 同时注册系统深浅切换跟随（ThemeChanged → Acrylic 重新着色）。
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
