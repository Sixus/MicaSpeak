//! M3 设置类 command：语音模式、PTT 按键、阈值、设备切换（docs/02 §4 合约）。
//! 全部走 config.json 持久化 + 热生效（只重建音频流，不重连服务器）。

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tracing::info;

use crate::app_state::AppState;

/// 改绑全局 PTT 按键（vk 为 Windows 虚拟键码）。
/// 同键重复绑定视为冲突，返回中文提示；成功后旧键立即注销。
#[tauri::command]
pub async fn set_ptt_key(
    app: AppHandle,
    state: State<'_, AppState>,
    vk: u32,
) -> Result<(), String> {
    if vk == 0 {
        return Err("无效的按键".into());
    }
    {
        let mut config = state.config.lock().await;
        if config.voice.ptt_key_vk == vk {
            return Err(format!("VK 0x{vk:02X} 已经是当前的 PTT 按键，请按其他键"));
        }
        config.voice.ptt_key_vk = vk;
        crate::persistence::save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    // 原子替换目标键：旧键立即失效（docs/07 A4）。
    crate::hotkey::set_target_vk(vk);
    info!(vk, "PTT 按键已改绑，旧键已注销");
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 打开设置窗口（520×640，按需创建；已存在则聚焦）。复用同一后端状态。
#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html".into()))
        .title("MicaSpeak 设置")
        .inner_size(520.0, 640.0)
        .min_inner_size(460.0, 520.0)
        .build()
        .map_err(|e| format!("打开设置窗口失败：{e}"))?;
    Ok(())
}
