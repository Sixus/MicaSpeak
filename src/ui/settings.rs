//! 设置窗口（独立 520×640 viewport）：左分类栏 + 右侧「发送」页。
//! MD2 阶段仅「发送」分类有内容（设计稿如此），其余分类占位到接线里程碑。
use crate::theme::{colors, font, metrics, Theme};
use crate::ui::icons::{self as icons, Icon};
use crate::ui::widgets::{self as w};
use eframe::egui::{self, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const CATEGORIES: [&str; 5] = ["发送", "音频", "悬浮窗", "身份", "关于"];

#[derive(Clone)]
pub struct SettingsState {
    pub category: usize,
    /// false = PTT，true = VAD
    pub vad_mode: bool,
    pub vad_threshold: f32,
    pub denoise: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        SettingsState {
            category: 0,
            vad_mode: false,
            vad_threshold: 0.6,
            denoise: true,
        }
    }
}

/// 设置窗口主体（viewport 回调内调用；标题栏在 app.rs 的窗口骨架里）。
pub fn show(ui: &mut Ui, t: &Theme, st: &mut SettingsState, overlay_open: &mut bool) {
    // 与主窗口相同的两栏模式：外层也必须显式占满剩余区域。
    // `horizontal` 会先按子项最小内容高度收缩，导致右页只剩标题高度。
    let row = ui.available_size();
    ui.allocate_ui_with_layout(row, egui::Layout::left_to_right(egui::Align::Min), |ui| {
        let gap = metrics::GAP;
        let margin = metrics::MARGIN;
        let h = row.y;
        let rail_w = metrics::SETTINGS_RAIL_W;
        // The two outer 12px margins and the 8px inter-column gap are part of
        // the 520px reference width. Keep the right panel at 360px instead of
        // silently leaving an 8px strip at the window edge.
        let panel_w = (ui.available_width() - margin * 2.0 - gap - rail_w).max(0.0);
        ui.add_space(margin);
        ui.allocate_ui_with_layout(
            Vec2::new(rail_w, h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_size(ui.available_size());
                rail(ui, t, st);
            },
        );
        ui.add_space(gap);
        ui.allocate_ui_with_layout(
            Vec2::new(panel_w, h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_size(ui.available_size());
                panel(ui, t, st, overlay_open);
            },
        );
        ui.add_space(margin);
    });
}

// ---------- 左侧分类栏 ----------

fn rail(ui: &mut Ui, t: &Theme, st: &mut SettingsState) {
    w::card(t, egui::Margin::same(6)).show(ui, |ui| {
        ui.set_width(metrics::SETTINGS_RAIL_W - 12.0);
        ui.set_min_size(ui.available_size());
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("设置")
                .size(font::RAIL_TITLE)
                .strong()
                .color(t.text),
        );
        ui.add_space(4.0);
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let selected = st.category == i;
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
            if selected {
                ui.painter().rect(
                    rect,
                    CornerRadius::same(metrics::RADIUS_CTRL as u8),
                    t.hover,
                    Stroke::NONE,
                    StrokeKind::Inside,
                );
                // 左侧 3×16 蓝色指示条
                let (bw, bh) = metrics::RAIL_INDICATOR;
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        egui::pos2(rect.left() + 2.0, rect.center().y),
                        Vec2::new(bw, bh),
                    ),
                    CornerRadius::same(2),
                    colors::BLUE,
                );
            } else if resp.hovered() {
                ui.painter().rect(
                    rect,
                    CornerRadius::same(metrics::RADIUS_CTRL as u8),
                    colors::ROW_HOVER,
                    Stroke::NONE,
                    StrokeKind::Inside,
                );
            }
            let galley = ui.painter().layout(
                cat.to_string(),
                FontId::proportional(font::BODY),
                if selected { t.text } else { t.subtext },
                f32::INFINITY,
            );
            ui.painter().galley(
                egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
                galley,
                if selected { t.text } else { t.subtext },
            );
            if resp.clicked() {
                st.category = i;
            }
        }
    });
}

// ---------- 右侧面板 ----------

fn panel(ui: &mut Ui, t: &Theme, st: &mut SettingsState, overlay_open: &mut bool) {
    let w = ui.available_width();
    let h = ui.available_height();
    w::card(t, egui::Margin::same(18)).show(ui, |ui| {
        // `w`/`h` are the card's outer allocation. Frame contents exclude
        // the 18px margins on both sides, matching CSS border-box sizing.
        let inner_w = (w - 36.0).max(0.0);
        let inner_h = (h - 36.0).max(0.0);
        ui.set_width(inner_w);
        ui.set_min_size(Vec2::new(inner_w, inner_h));
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .id_salt("settings_scroll")
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if st.category == 0 {
                    tx_page(ui, t, st);
                } else if st.category == 2 {
                    overlay_page(ui, t, overlay_open);
                } else {
                    // 其余分类：接线里程碑补内容
                    let name = CATEGORIES[st.category];
                    ui.label(
                        egui::RichText::new(name)
                            .size(font::PAGE_TITLE)
                            .strong()
                            .color(t.text),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "该分类在后续接线里程碑补齐（MD2 仅实现设计稿定义的「发送」页）。",
                        )
                        .size(font::AUX)
                        .color(t.subtext),
                    );
                }
            });
    });
}

/// 「悬浮窗」页：设置窗口是产品入口，开关直接控制独立置顶 viewport。
fn overlay_page(ui: &mut Ui, t: &Theme, overlay_open: &mut bool) {
    ui.label(
        egui::RichText::new("悬浮窗")
            .size(font::PAGE_TITLE)
            .strong()
            .color(t.text),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("在桌面上显示当前说话者。")
            .size(font::AUX)
            .color(t.subtext),
    );
    ui.add_space(18.0);
    toggle_row(ui, t, "显示悬浮窗", "独立置顶窗口", overlay_open);
}

fn section_title(ui: &mut Ui, t: &Theme, text: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text)
            .size(font::SECTION)
            .strong()
            .color(t.faint),
    );
    ui.add_space(10.0);
}

fn divider(ui: &mut Ui, t: &Theme) {
    ui.add_space(16.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, t.divider),
    );
    ui.add_space(16.0);
}

/// 「发送」页。
fn tx_page(ui: &mut Ui, t: &Theme, st: &mut SettingsState) {
    ui.label(
        egui::RichText::new("发送")
            .size(font::PAGE_TITLE)
            .strong()
            .color(t.text),
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("控制语音的传输方式与降噪。")
            .size(font::AUX)
            .color(t.subtext),
    );
    ui.add_space(18.0);

    section_title(ui, t, "传输模式");
    radio_row(
        ui,
        t,
        !st.vad_mode,
        "按键说话 (PTT)",
        &mut st.vad_mode,
        false,
    );
    radio_row(ui, t, st.vad_mode, "语音激活 (VAD)", &mut st.vad_mode, true);

    // 快捷键行
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(28.0);
        ui.label(
            egui::RichText::new("快捷键")
                .size(font::CTRL_ROW)
                .color(t.subtext),
        );
        ui.add_space(10.0);
        // 键位 chip
        let galley = ui.painter().layout(
            "左Ctrl".to_owned(),
            FontId::proportional(font::CTRL_ROW),
            t.text,
            f32::INFINITY,
        );
        let chip = Vec2::new(galley.size().x + 20.0, 25.0);
        let (rect, _) = ui.allocate_exact_size(chip, Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(metrics::RADIUS_CTRL as u8),
            t.chip,
            Stroke::new(1.0, t.input_border),
            StrokeKind::Inside,
        );
        ui.painter().galley(
            egui::pos2(
                rect.center().x - galley.size().x / 2.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            t.text,
        );
        ui.add_space(10.0);
        let _ = w::secondary_button(ui, t, "修改", metrics::BTN_SMALL_H); // M3 接热键录制
    });
    ui.add_space(6.0);

    divider(ui, t);

    // VAD 阈值滑条
    section_title(ui, t, "语音激活阈值");
    slider_row(ui, t, st);
    ui.add_space(10.0);
    vad_bar(ui, t, st);
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("实时语音概率 · 超过阈值即开始传输")
            .size(font::SMALL)
            .color(t.subtext),
    );

    // 降噪开关
    ui.add_space(16.0);
    toggle_row(ui, t, "噪声抑制", "AI 降噪 (RNNoise)", &mut st.denoise);
    ui.add_space(4.0);

    divider(ui, t);

    section_title(ui, t, "音频设备");
    dropdown_row(ui, t, "输入设备", "系统默认");
    ui.add_space(12.0);
    dropdown_row(ui, t, "输出设备", "系统默认");

    ui.add_space(20.0);
    if w::text_link(ui, "恢复默认设置") {
        *st = SettingsState::default();
    }
}

fn radio_row(ui: &mut Ui, t: &Theme, checked: bool, label: &str, target: &mut bool, value: bool) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let c = egui::pos2(rect.left() + 9.0, rect.center().y);
    ui.painter().circle_stroke(
        c,
        metrics::RADIO / 2.0,
        Stroke::new(
            1.5,
            if checked {
                colors::BLUE
            } else {
                t.input_border
            },
        ),
    );
    if checked {
        ui.painter().circle_filled(c, 4.0, colors::BLUE);
    }
    let galley = ui.painter().layout(
        label.to_owned(),
        FontId::proportional(font::BODY),
        t.text,
        f32::INFINITY,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    if resp.clicked() {
        *target = value;
    }
}

fn slider_row(ui: &mut Ui, t: &Theme, st: &mut SettingsState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("0.1")
                .size(font::SECTION)
                .color(t.faint),
        );
        ui.add_space(12.0);
        // Keep both range labels in the row. The prototype reserves the same
        // 0.9 label width as the current value, so the track never shifts when
        // the value changes.
        let track_w = (ui.available_width() - 112.0).max(48.0);
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(track_w, 20.0), Sense::click_and_drag());
        if resp.dragged() || resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let frac = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                st.vad_threshold = ((0.1 + frac * 0.8) * 100.0).round() / 100.0;
            }
        }
        let frac = ((st.vad_threshold - 0.1) / 0.8).clamp(0.0, 1.0);
        // 轨道
        let track = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.center().y),
            Vec2::new(rect.width(), 4.0),
        );
        ui.painter().rect(
            track,
            CornerRadius::same(2),
            t.chip,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        let fill = Rect::from_min_max(
            track.left_center(),
            egui::pos2(track.left() + track.width() * frac, track.center().y + 2.0),
        );
        ui.painter().rect(
            fill,
            CornerRadius::same(2),
            colors::BLUE,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        // 把手
        let knob_c = egui::pos2(track.left() + track.width() * frac, track.center().y);
        ui.painter()
            .circle_filled(knob_c, metrics::SLIDER_KNOB / 2.0, colors::WHITE);
        ui.painter().circle_stroke(
            knob_c,
            metrics::SLIDER_KNOB / 2.0,
            Stroke::new(1.0, t.input_border),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("0.9")
                .size(font::SECTION)
                .color(t.faint),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{:.1}", st.vad_threshold))
                .size(font::CTRL_ROW)
                .strong()
                .color(t.text),
        );
    });
}

/// 实时语音概率条（演示动画；接线时换成 nnnoiseless 真实概率）。
fn vad_bar(ui: &mut Ui, t: &Theme, _st: &SettingsState) {
    let time = ui.ctx().time() as f32;
    let level = 0.48 + 0.22 * (time * 2.6).sin();
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), metrics::VAD_BAR_H),
        Sense::hover(),
    );
    ui.painter().rect(
        Rect::from_center_size(rect.center(), Vec2::new(rect.width(), metrics::VAD_BAR_H)),
        CornerRadius::same(metrics::VAD_BAR_H as u8 / 2),
        t.chip,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    let fill_w = rect.width() * level;
    let n = 5;
    for i in 0..n {
        let x0 = rect.left() + fill_w * i as f32 / n as f32;
        let x1 = rect.left() + fill_w * (i + 1) as f32 / n as f32;
        let tt = (i as f32 + 0.5) / n as f32;
        let c = lerp_color(colors::BLUE, colors::METER_GREEN, tt);
        ui.painter().rect_filled(
            Rect::from_min_max(
                egui::pos2(x0, rect.top()),
                egui::pos2(x1.min(rect.right()), rect.bottom()),
            ),
            CornerRadius::same(0),
            c,
        );
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(60));
}

fn toggle_row(ui: &mut Ui, t: &Theme, title: &str, sub: &str, value: &mut bool) {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
    let g1 = ui.painter().layout(
        title.to_owned(),
        FontId::proportional(font::BODY),
        t.text,
        f32::INFINITY,
    );
    let g2 = ui.painter().layout(
        sub.to_owned(),
        FontId::proportional(font::SMALL),
        t.subtext,
        f32::INFINITY,
    );
    ui.painter()
        .galley(egui::pos2(rect.left() + 2.0, rect.top() + 2.0), g1, t.text);
    ui.painter().galley(
        egui::pos2(rect.left() + 2.0, rect.top() + 22.0),
        g2,
        t.subtext,
    );
    // 开关胶囊
    let (tw, th) = metrics::TOGGLE;
    let tg = Rect::from_center_size(
        egui::pos2(rect.right() - tw / 2.0 - 2.0, rect.center().y),
        Vec2::new(tw, th),
    );
    let on = *value;
    ui.painter().rect(
        tg,
        CornerRadius::same(th as u8 / 2),
        if on { colors::BLUE } else { t.chip },
        Stroke::new(1.0, if on { colors::BLUE } else { t.input_border }),
        StrokeKind::Inside,
    );
    let knob_x = if on {
        tg.right() - 8.0 - 2.0
    } else {
        tg.left() + 2.0 + 8.0
    };
    ui.painter().circle_filled(
        egui::pos2(knob_x, tg.center().y),
        8.0,
        if on { colors::WHITE } else { t.subtext },
    );
    if resp.clicked() {
        *value = !*value;
    }
}

fn dropdown_row(ui: &mut Ui, t: &Theme, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).size(font::AUX).color(t.subtext));
    ui.add_space(5.0);
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), metrics::INPUT_H),
        Sense::click(),
    );
    let bg = if resp.hovered() { t.hover } else { t.input_bg };
    ui.painter().rect(
        rect,
        CornerRadius::same(metrics::RADIUS_CTRL as u8),
        bg,
        Stroke::new(1.0, t.input_border),
        StrokeKind::Inside,
    );
    let galley = ui.painter().layout(
        value.to_owned(),
        FontId::proportional(font::BODY),
        t.text,
        f32::INFINITY,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    icons::draw(
        ui.painter(),
        egui::pos2(rect.right() - 16.0, rect.center().y),
        12.0,
        Icon::ChevronDown,
        t.faint,
    );
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
