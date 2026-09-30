//! M3 设置类 command：语音模式、PTT 按键、阈值、设备切换（docs/02 §4 合约）。
//! 全部走 config.json 持久化 + 热生效（只重建音频流，不重连服务器）。

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::info;

use crate::app_state::AppState;
use crate::persistence::save_config;

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

/// UI 改版：设置已集成进主窗口，不再创建独立设置窗口。
/// 显示并聚焦主窗口，广播 app://open-settings 让前端切到内置设置视图。
/// 托盘"打开设置"调用；前端 ⚙/连接页入口直接走页内切换，不经过这里。
pub fn open_settings(app: &AppHandle) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    let win = app.get_webview_window("main").ok_or("主窗口不存在")?;
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    app.emit_to("main", "app://open-settings", ())
        .map_err(|e| format!("通知主窗口打开设置失败：{e}"))
}

/// 设置/清除服务器备注（三轮：键=连接地址字符串，空串=清除）。
/// 用于主界面顶栏显示与收藏名（"备注·昵称"）。
#[tauri::command]
pub async fn set_server_remark(
    app: AppHandle,
    state: State<'_, AppState>,
    address: String,
    remark: String,
) -> Result<(), String> {
    let remark = remark.trim().to_string();
    {
        let mut config = state.config.lock().await;
        if remark.is_empty() {
            config.server_remarks.remove(&address);
        } else {
            config.server_remarks.insert(address, remark);
        }
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 修改语音模式与/或降噪开关；非 None 的字段才更新。
/// 热生效：保存配置 → 同步参数 → 只重建音频流（不重连服务器）。
#[tauri::command]
pub async fn set_voice_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: Option<String>,
    denoise: Option<bool>,
) -> Result<(), String> {
    if let Some(m) = &mode {
        if m != "ptt" && m != "vad" {
            return Err("无效的语音模式，只支持 ptt 或 vad".into());
        }
    }
    let (mode_vad, denoise_now) = {
        let mut config = state.config.lock().await;
        if let Some(m) = mode {
            config.voice.mode = m;
        }
        if let Some(d) = denoise {
            config.voice.denoise = d;
        }
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
        (config.voice.mode == "vad", config.voice.denoise)
    };
    // 模式/降噪是原子量：即时生效，无需重建音频流。
    state.audio.set_mode_denoise(mode_vad, denoise_now);
    info!("语音设置已即时生效");
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 修改 VAD 阈值（0.1~0.9），热生效同上。
#[tauri::command]
pub async fn set_vad_threshold(
    app: AppHandle,
    state: State<'_, AppState>,
    value: f32,
) -> Result<(), String> {
    if !(0.1..=0.9).contains(&value) {
        return Err("阈值需在 0.1 到 0.9 之间".into());
    }
    {
        let mut config = state.config.lock().await;
        config.voice.vad_threshold = value;
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    // 阈值是原子量：拖动滑条即时生效，无需重建音频流。
    state.audio.set_vad_threshold(value);
    state.emit_snapshot(&app).await;
    Ok(())
}

#[derive(Serialize)]
pub struct DeviceList {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// 枚举系统输入/输出设备名（cpal），供设置页下拉框。
#[tauri::command]
pub async fn list_audio_devices() -> Result<DeviceList, String> {
    // cpal 枚举是阻塞调用，放到独立线程避免占住异步执行器。
    tauri::async_runtime::spawn_blocking(|| {
        let host = cpal::default_host();
        let input_names = || -> Vec<String> {
            let Ok(devs) = host.input_devices() else { return Vec::new() };
            devs.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect()
        };
        let output_names = || -> Vec<String> {
            let Ok(devs) = host.output_devices() else { return Vec::new() };
            devs.filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect()
        };
        DeviceList { inputs: input_names(), outputs: output_names() }
    })
    .await
    .map_err(|e| format!("枚举设备失败：{e}"))
}

/// 切换输入/输出设备（None = 系统默认）；重建音频流，不重连服务器。
#[tauri::command]
pub async fn set_audio_devices(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Option<String>,
    output: Option<String>,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().await;
        config.voice.input_device = input;
        config.voice.output_device = output;
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    let params = state.audio_devices_from_config().await;
    state.audio.set_voice_params(params);
    // 只有设备变更需要重建流；模式/阈值/降噪走原子量即时生效。
    state.audio.rebuild();
    state.emit_snapshot(&app).await;
    Ok(())
}
