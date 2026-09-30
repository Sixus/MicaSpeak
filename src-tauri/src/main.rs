#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod app_state;
mod audio;
mod chat;
mod conn;
mod hotkey;
mod identity;
mod logging;
mod material;
mod overlay;
mod persistence;
mod registry;
mod settings;
mod tray;

use std::sync::atomic::Ordering;

use app_state::AppState;
use conn::{connect, disconnect, get_app_snapshot, reconnect, select_channel};
use persistence::{delete_bookmark, save_bookmark};
use settings::{list_audio_devices, set_audio_devices, set_ptt_key, set_server_remark, set_vad_threshold, set_voice_mode};
use tauri::Manager;

fn main() {
    // M5a：文件+stderr 双写日志先行（冷启动打点、重连退避等验收证据依赖它）。
    logging::init();
    // M5c：WebView2 用户数据目录固定到数据根的 WebView2/（绿色目录=exe 旁 Data/）。
    // 环境变量是 WebView2 加载器的权威路径，双击启动时 cwd 不可预测，conf 里的
    // 相对路径不可靠。必须在任何 Webview 创建前设置。
    let webview_data = crate::persistence::data_root().join("WebView2");
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview_data);
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            connect,
            disconnect,
            reconnect,
            select_channel,
            chat::send_channel_message,
            chat::send_private_message,
            chat::open_private_chat,
            chat::close_chat_tab,
            chat::open_url,
            overlay::set_overlay_enabled,
            overlay::set_overlay_editing,
            overlay::save_overlay_position,
            audio::set_transmit_enabled,
            set_ptt_key,
            set_server_remark,
            set_voice_mode,
            set_vad_threshold,
            list_audio_devices,
            set_audio_devices,
            save_bookmark,
            delete_bookmark,
            identity::import_identity,
            identity::create_identity,
            identity::delete_identity,
            identity::set_active_identity,
            identity::export_identity,
            identity::start_security_upgrade,
            identity::cancel_security_upgrade,
            about::get_about_info,
            about::open_licenses
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();
            // M5a：主窗口材质（Mica/实体回退）+ 系统深浅跟随。
            if let Err(e) = material::init_main_window_material(&state.material, &handle) {
                log::warn!("窗口材质初始化失败：{e}");
            }
            // M5 B1：托盘。失败走降级预案（关闭=退出、最小化=任务栏），不中断启动。
            if let Err(e) = tray::init(&handle) {
                log::warn!("托盘初始化失败，启用降级预案（关闭=退出）：{e}");
            }
            // M5 B1：托盘可用时主窗口"关闭 = 隐藏到托盘"（保持连接）；降级时放行关闭。
            if let Some(main_win) = handle.get_webview_window("main") {
                let win = main_win.clone();
                main_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if tray::TRAY_OK.load(Ordering::Relaxed) {
                            api.prevent_close();
                            let _ = win.hide();
                            log::info!("主窗口已隐藏到托盘（连接保持）");
                        }
                    }
                });
            }
            // M3 A：全局 PTT 低级键盘钩子。目标键取自配置，钩子线程常驻消息泵。
            let vk = persistence::load_config()
                .map(|(config, _)| config.voice.ptt_key_vk)
                .unwrap_or(0xA2);
            hotkey::set_target_vk(vk);
            hotkey::spawn_hook_thread();
            tauri::async_runtime::spawn(hotkey::watch_task(state.audio.clone()));
            // M4c：悬浮窗——按需创建（首次开启/编辑时 ensure_overlay_window）。
            // M5c C4 内存优化：默认关闭时不再常驻一个隐藏 renderer（实测省
            // ~30-45MB 专用内存），显隐任务对窗口缺失有容忍（循环等待）。
            let (ov_handle, ov_state) = (handle.clone(), state.clone());
            tauri::async_runtime::spawn(overlay::overlay_task(ov_handle, ov_state));
            // M2 音频事件任务：转发/限频推送/健康重建（任务必须被 Tokio 实际轮询，
            // 见 M0 教训）。全部消费 AppState 原子量与通道，不碰音频回调。
            let (relay_handle, relay_state) = (handle.clone(), state.clone());
            tauri::async_runtime::spawn(audio::event_relay_task(relay_handle, relay_state));
            let (level_handle, level_state) = (handle.clone(), state.clone());
            tauri::async_runtime::spawn(audio::level_task(level_handle, level_state));
            let (init_handle, init_state) = (handle.clone(), state.clone());
            tauri::async_runtime::spawn(async move {
                init_state.emit_initial(&init_handle).await;
            });
            tauri::async_runtime::spawn(audio::health_task(handle.clone(), state));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("MicaSpeak failed to start");
}
