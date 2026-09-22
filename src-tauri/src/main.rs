#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod conn;
mod persistence;

use app_state::AppState;
use conn::{connect, disconnect, get_app_snapshot, reconnect, select_channel};
use persistence::{delete_bookmark, save_bookmark};
use tauri::Manager;

fn main() {
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
