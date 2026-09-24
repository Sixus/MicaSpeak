//! M3 设置类 command：语音模式、PTT 按键、阈值、设备切换（docs/02 §4 合约）。
//! 全部走 config.json 持久化 + 热生效（只重建音频流，不重连服务器）。

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
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
