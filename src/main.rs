#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;
mod ui;

use app::MicaApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MicaSpeak")
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([360.0, 520.0])
            .with_resizable(true)
            .with_decorations(false)
            .with_transparent(true),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "MicaSpeak",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(MicaApp::new(cc)))
        }),
    )
}

/// 字体挂载：拉丁优先 Segoe UI Variable（部分精简安装缺失则回退 Segoe UI），
/// 中文回退微软雅黑。egui 按族内顺序取字形，拉丁在前保证英文用 Segoe 渲染。
fn install_fonts(ctx: &egui::Context) {
    let latin = find_font(&["segoeuivari"], "segoeui.ttf");
    let cjk = find_font(&[], "msyh.ttc")
        .or_else(|| find_font(&[], "Deng.ttf"))
        .or_else(|| find_font(&[], "simhei.ttf"));

    let mut fonts = egui::FontDefinitions::default();
    let mut next_slot = 0usize;
    if let Some(bytes) = latin {
        fonts.font_data.insert(
            "micaspeak-latin".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(next_slot, "micaspeak-latin".to_owned());
        next_slot += 1;
    }
    if let Some(bytes) = cjk {
        fonts.font_data.insert(
            "micaspeak-cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(next_slot, "micaspeak-cjk".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("micaspeak-cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// 在 C:\Windows\Fonts 里找前缀匹配的 .ttf（取排序后第一个），或按精确文件名取。
fn find_font(prefixes: &[&str], exact: &str) -> Option<Vec<u8>> {
    let dir = std::path::Path::new(r"C:\Windows\Fonts");
    if let Some(bytes) = try_read(dir.join(exact)) {
        return Some(bytes);
    }
    let mut hits: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_owned()))
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".ttf") && prefixes.iter().any(|p| lower.starts_with(p))
        })
        .collect();
    hits.sort();
    hits.first().and_then(|name| try_read(dir.join(name)))
}

fn try_read(path: std::path::PathBuf) -> Option<Vec<u8>> {
    if path.exists() {
        std::fs::read(path).ok()
    } else {
        None
    }
}
