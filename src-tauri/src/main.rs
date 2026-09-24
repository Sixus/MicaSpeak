#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod audio;
mod chat;
mod conn;
mod hotkey;
mod overlay;
mod persistence;
mod settings;

use app_state::AppState;
use conn::{connect, disconnect, get_app_snapshot, reconnect, select_channel};
use persistence::{delete_bookmark, save_bookmark};
use settings::{list_audio_devices, open_settings, set_audio_devices, set_ptt_key, set_vad_threshold, set_voice_mode};
use tauri::Manager;

/// 极简 stderr 日志：设置 MICASPEAK_LOG=debug/trace 时启用，
/// 用于查看 tsclientlib/tsproto 的协议级诊断（不含密码与私钥）。
struct StderrLogger(log::LevelFilter);

impl log::Log for StderrLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        meta.level() <= self.0
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[{:>5}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

fn init_logging() {
    let Ok(level) = std::env::var("MICASPEAK_LOG") else {
        return;
    };
    let (tmax, lmax) = match level.to_ascii_lowercase().as_str() {
        "trace" => (
            tracing_subscriber::filter::LevelFilter::TRACE,
            log::LevelFilter::Trace,
        ),
        "debug" => (
            tracing_subscriber::filter::LevelFilter::DEBUG,
            log::LevelFilter::Debug,
        ),
        "info" => (
            tracing_subscriber::filter::LevelFilter::INFO,
            log::LevelFilter::Info,
        ),
        "warn" => (
            tracing_subscriber::filter::LevelFilter::WARN,
            log::LevelFilter::Warn,
        ),
        _ => (
            tracing_subscriber::filter::LevelFilter::ERROR,
            log::LevelFilter::Error,
        ),
    };
    // tsclientlib/tsproto 走 tracing，tauri 走 log crate；两套都接上。
    tracing_subscriber::fmt()
        .with_max_level(tmax)
        .with_writer(std::io::stderr)
        .init();
    let _ = log::set_boxed_logger(Box::new(StderrLogger(lmax)));
    log::set_max_level(lmax);
}

fn main() {
    init_logging();
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
            open_settings,
            set_voice_mode,
            set_vad_threshold,
            list_audio_devices,
            set_audio_devices,
            save_bookmark,
            delete_bookmark
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();
            // M3 A：全局 PTT 低级键盘钩子。目标键取自配置，钩子线程常驻消息泵。
            let vk = persistence::load_config()
                .map(|(config, _)| config.voice.ptt_key_vk)
                .unwrap_or(0xA2);
            hotkey::set_target_vk(vk);
            hotkey::spawn_hook_thread();
            tauri::async_runtime::spawn(hotkey::watch_task(state.audio.clone()));
            // M4c：悬浮窗——启动即创建（隐藏），显隐任务按 TalkingState 驱动。
            {
                let (o_handle, o_state) = (handle.clone(), state.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = overlay::ensure_overlay_window(&o_handle, &o_state).await {
                        log::warn!("悬浮窗创建失败：{e}");
                    }
                });
            }
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
            tauri::async_runtime::spawn(audio::health_task(state));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("MicaSpeak failed to start");
}
