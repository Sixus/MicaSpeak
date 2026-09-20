#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MicaSpeak")
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([360.0, 520.0])
            .with_resizable(true),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "MicaSpeak",
        options,
        Box::new(|cc| {
            install_system_cjk_font(&cc.egui_ctx);
            Ok(Box::new(MicaApp::default()))
        }),
    )
}

#[derive(Default)]
struct MicaApp;

impl eframe::App for MicaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 2.0 - 16.0);
                ui.heading("MicaSpeak M0 骨架就绪");
            });
        });
    }
}

/// egui 默认字体不含 CJK 字形，中文会渲染成方块；按优先级挂载系统中文字体，
/// 全部缺失时保持默认字体（仅影响文字显示，不影响骨架功能）。
fn install_system_cjk_font(ctx: &egui::Context) {
    const CJK_FONT_CANDIDATES: [&str; 3] = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
    ];

    let Some(path) = CJK_FONT_CANDIDATES
        .iter()
        .find(|p| std::path::Path::new(p).exists())
    else {
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(
            "micaspeak-cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "micaspeak-cjk".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("micaspeak-cjk".to_owned());
    ctx.set_fonts(fonts);
}
