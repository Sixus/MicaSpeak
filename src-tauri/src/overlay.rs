//! M4c 悬浮窗：独立 WebviewWindow（透明/无边框/置顶/默认穿透），
//! 显隐由 TalkingState 驱动（有人说话显示、停止约 1 秒隐藏），
//! 位置由编辑模式拖动后经 save_overlay_position 持久化（docs/08 阶段 C）。

use crate::app_state::AppState;
use crate::persistence::save_config;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tracing::info;

/// 悬浮窗会话事实（enabled 持久化在 config，editing 是瞬态）。
#[derive(Clone, Default)]
pub struct OverlayState {
    editing: Arc<AtomicBool>,
    /// 上次广播的可见性，避免重复 show/hide。
    visible: Arc<AtomicBool>,
}

impl OverlayState {
    pub fn editing(&self) -> bool {
        self.editing.load(Ordering::Relaxed)
    }
}

/// 启动时创建悬浮窗（默认隐藏、不抢焦点）。重复调用安全（已存在直接返回）。
pub async fn ensure_overlay_window(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }
    let (x, y) = {
        let config = state.config.lock().await;
        (config.overlay.x, config.overlay.y)
    };
    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html".into()))
        .title("MicaSpeak 悬浮窗")
        .inner_size(260.0, 80.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
        .map_err(|e| format!("创建悬浮窗失败：{e}"))?;
    // 保存的是 outerPosition 的物理像素；builder.position 是逻辑坐标，
    // DPI 缩放下会漂移——这里显式按物理坐标恢复。
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    // 默认鼠标穿透；编辑模式时由命令解除。
    apply_pass_through(app, state)?;
    info!("悬浮窗已创建（隐藏）");
    Ok(())
}

/// 穿透状态跟随编辑模式：平时穿透，编辑时可点击/拖动。
fn apply_pass_through(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let Some(win) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let editing = state.overlay.editing();
    win.set_ignore_cursor_events(!editing)
        .map_err(|e| format!("设置鼠标穿透失败：{e}"))
}

/// 显隐 + overlay://state 驱动任务：轮询说话人状态（复用 M2/M3 的 talking 事实），
/// 有人说话显示、停止约 1 秒（说话清扫周期）隐藏；编辑模式下强制显示。
pub async fn overlay_task(app: AppHandle, state: AppState) {
    let mut last_signature = String::new();
    loop {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let enabled = {
            let config = state.config.lock().await;
            config.overlay.enabled
        };
        let editing = state.overlay.editing();
        let talkers = state.talking_list();
        // 编辑模式强制显示（否则无人说话时隐藏）。
        let visible = enabled && (editing || !talkers.is_empty());
        // M5c：窗口按需创建——需要显示（或上轮会话开着）而窗口不存在时补建。
        if app.get_webview_window("overlay").is_none() {
            if enabled || editing {
                if let Err(e) = ensure_overlay_window(&app, &state).await {
                    log::warn!("悬浮窗创建失败：{e}");
                }
            } else {
                continue;
            }
        }
        let Some(win) = app.get_webview_window("overlay") else {
            continue;
        };

        let names: Vec<String> = talkers
            .iter()
            .map(|t| {
                if t.is_self {
                    format!("{}（我）", t.name)
                } else {
                    t.name.clone()
                }
            })
            .collect();
        let signature = format!("{}|{}|{:?}", visible, editing, names);
        if signature == last_signature {
            continue;
        }
        if visible != state.overlay.visible.load(Ordering::Relaxed) {
            if visible {
                let _ = win.show();
                apply_pass_through(&app, &state).ok();
            } else {
                let _ = win.hide();
            }
            state.overlay.visible.store(visible, Ordering::Relaxed);
        }
        let _ = tauri::Emitter::emit(
            &app,
            "overlay://state",
            serde_json::json!({ "visible": visible, "editing": editing, "talkers": names }),
        );
        last_signature = signature;
    }
}

// ---- Tauri commands（docs/02 §4 合约） ----

/// 开关悬浮窗：持久化到 config，并立即驱动显隐。
#[tauri::command]
pub async fn set_overlay_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    ensure_overlay_window(&app, &state).await?;
    {
        let mut config = state.config.lock().await;
        config.overlay.enabled = enabled;
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    // 任务循环会在下一个周期收敛显隐与事件；这里即时推一次减少延迟。
    if !enabled {
        if let Some(win) = app.get_webview_window("overlay") {
            let _ = win.hide();
        }
        state.overlay.visible.store(false, Ordering::Relaxed);
    }
    info!(enabled, "悬浮窗开关已切换");
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 编辑模式：解除穿透并强制显示，用户用系统拖动调整位置；退出时保存位置。
#[tauri::command]
pub async fn set_overlay_editing(
    app: AppHandle,
    state: State<'_, AppState>,
    editing: bool,
) -> Result<(), String> {
    ensure_overlay_window(&app, &state).await?;
    state.overlay.editing.store(editing, Ordering::Relaxed);
    apply_pass_through(&app, &state)?;
    if editing {
        if let Some(win) = app.get_webview_window("overlay") {
            let _ = win.show();
            state.overlay.visible.store(true, Ordering::Relaxed);
        }
    }
    info!(editing, "悬浮窗编辑模式");
    state.emit_snapshot(&app).await;
    Ok(())
}

/// 保存悬浮窗位置（编辑拖动结束后由 overlay 页面回读自身位置上报）。
#[tauri::command]
pub async fn save_overlay_position(
    state: State<'_, AppState>,
    x: i32,
    y: i32,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().await;
        config.overlay.x = x;
        config.overlay.y = y;
        save_config(&config).map_err(|e| format!("保存配置失败：{e}"))?;
    }
    info!(x, y, "悬浮窗位置已保存");
    Ok(())
}
