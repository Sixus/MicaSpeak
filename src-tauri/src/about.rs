//! M6c 关于页数据与许可入口（docs/02 §9：非官方声明、版本、构建日期、许可）。

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app_state::AppState;

#[derive(Serialize)]
pub struct AboutInfo {
    pub version: String,
    pub build_date: &'static str,
    pub webview2_version: Option<String>,
    pub webview2_available: bool,
    /// 免责声明的完整文案（关于页直接渲染）。
    pub disclaimer: &'static str,
}

pub const DISCLAIMER: &str =
    "MicaSpeak 是非官方的 TeamSpeak 3 第三方客户端，与 TeamSpeak Systems GmbH 无关联。\
“TeamSpeak”仅用于兼容性说明。本应用按现状提供，不附带任何担保。";

#[tauri::command]
pub async fn get_about_info(app: AppHandle, state: State<'_, AppState>) -> Result<AboutInfo, String> {
    let package = app.package_info();
    Ok(AboutInfo {
        version: package.version.to_string(),
        build_date: env!("BUILD_DATE"),
        webview2_version: state.webview2.version.clone(),
        webview2_available: state.webview2.available,
        disclaimer: DISCLAIMER,
    })
}

/// 打开绿色目录旁的 LICENSES 文件夹（资源管理器）。开发模式没有打包
/// LICENSES 时返回中文提示，不 panic。
#[tauri::command]
pub async fn open_licenses(app: AppHandle) -> Result<(), String> {
    let _ = app;
    let exe = std::env::current_exe().map_err(|e| format!("无法定位程序目录：{e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法定位程序目录".to_string())?
        .join("LICENSES");
    if !dir.is_dir() {
        return Err(format!("许可目录不存在（{}）。打包版才有 LICENSES 目录。", dir.display()));
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(dir.as_os_str())
            .spawn()
            .map_err(|e| format!("打开许可目录失败：{e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(format!("请手动打开目录：{}", dir.display()))
    }
}
