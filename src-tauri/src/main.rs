#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod conn;
mod persistence;

use app_state::AppState;
use conn::{connect, disconnect, get_app_snapshot, reconnect, select_channel};
use persistence::{delete_bookmark, save_bookmark};
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
            save_bookmark,
            delete_bookmark
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                state.emit_initial(&handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("MicaSpeak failed to start");
}
