//! M5 B1：系统托盘（docs/09 阶段 B）。tauri 内置 tray-icon（Cargo.toml 已启用
//! feature，非插件）。左键单击 = 显示主窗口；右键菜单：显示主窗口 / 打开设置 /
//! 退出。
//!
//! 关闭按钮行为：托盘可用时"关闭 = 隐藏到托盘"（语音客户端惯例，保持连接）；
//! 托盘初始化失败时按任务卡降级预案收敛：关闭 = 退出、最小化 = 任务栏，
//! 由 TRAY_OK 原子量控制（setup 里注册的 CloseRequested 处理读取它）。

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

/// 托盘是否可用（降级预案开关）。false 时主窗口关闭即退出。
pub static TRAY_OK: AtomicBool = AtomicBool::new(false);

pub fn init(app: &AppHandle) -> Result<(), String> {
    let show = MenuItem::with_id(app, "tray-show", "显示主窗口", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let settings = MenuItem::with_id(app, "tray-settings", "打开设置", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "tray-quit", "退出", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&show, &settings, &quit]).map_err(|e| e.to_string())?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("应用没有内嵌图标（bundle.icon 未配置）")?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("MicaSpeak")
        .menu(&menu)
        // 左键留给"显示主窗口"，右键才弹菜单
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-show" => show_main(app),
            "tray-settings" => {
                // UI 改版：设置集成进主窗口——聚焦主窗口并发事件切到内置设置视图。
                if let Err(e) = crate::settings::open_settings(app) {
                    log::warn!("打开设置失败：{e}");
                }
            }
            "tray-quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| format!("创建托盘失败：{e}"))?;
    TRAY_OK.store(true, Ordering::Relaxed);
    Ok(())
}

/// 显示并聚焦主窗口（托盘左键/菜单共用）。
fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}
