//! 共享控件：卡片、状态点、Fluent 输入框、按钮、窗口底渐变。
//! 全部视觉参数取自 src/theme.rs（设计令牌）。

#![allow(dead_code)] // MD2 阶段控件逐步接入
use crate::theme::{colors, font, metrics, Theme};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::icons::{self as icons, Icon};

/// 状态点种类（顶栏连接状态）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotKind {
    Green,
    Yellow,
    Red,
    Gray,
}

impl DotKind {
    fn color(self) -> Color32 {
        match self {
            DotKind::Green => colors::GREEN,
            DotKind::Yellow => colors::YELLOW,
            DotKind::Red => colors::RED,
            DotKind::Gray => colors::GRAY,
        }
    }
}

/// 8px 实心圆点 + 3px 同色光晕，画在 pos（圆心）。
pub fn status_dot(ui: &Ui, pos: Pos2, kind: DotKind) {
    let c = kind.color();
    ui.painter().circle_filled(pos, metrics::STATUS_DOT / 2.0, c);
    let halo = Color32::from_rgba_unmultiplied_const(c.r(), c.g(), c.b(), 0x22);
    ui.painter().circle(
        pos,
        metrics::STATUS_DOT / 2.0 + 1.5,
        Color32::TRANSPARENT,
        Stroke::new(3.0, halo),
    );
}

/// 内容卡片：86% 不透明、8px 圆角、1px 描边。调用方给内边距。
pub fn card(t: &Theme, pad: Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(metrics::RADIUS_CARD as u8))
        .inner_margin(pad)
}

/// Fluent 单行输入框：34px 高、5px 圆角、2px 加粗下边线；error 红；聚焦变蓝。
/// 点击框内空白也能聚焦输入区。返回 TextEdit 的响应。
pub fn fluent_input(ui: &mut Ui, t: &Theme, label: &str, text: &mut String, error: bool) -> Response {
    ui.label(egui::RichText::new(label).size(font::AUX).color(t.subtext));
    ui.add_space(3.0);
    let width = ui.available_width();
    let (rect, box_resp) =
        ui.allocate_exact_size(Vec2::new(width, metrics::INPUT_H), Sense::click());
    let border = if error { colors::RED } else { t.input_border };
    ui.painter().rect(
        rect,
        CornerRadius::same(metrics::RADIUS_CTRL as u8),
        t.input_bg,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let inner_w = (rect.width() - 20.0).max(20.0);
    let inner = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.center().y - 11.0),
        Vec2::new(inner_w, 22.0),
    );
    let edit = egui::TextEdit::singleline(text)
        .frame(egui::Frame::new())
        .desired_width(inner_w)
        .font(FontId::proportional(font::BODY))
        .text_color(t.text)
        .vertical_align(egui::Align::Center);
    let edit_resp = ui.put(inner, edit);

    // 加粗下边线：常态灰、聚焦蓝、错误红（在文字之后画，位于框底不压字）
    let bottom = if error {
        colors::RED
    } else if edit_resp.has_focus() {
        colors::BLUE
    } else {
        t.input_bottom
    };
    let band = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.bottom() - 3.0),
        Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0),
    );
    ui.painter().rect_filled(band, CornerRadius::same(1), bottom);
    if box_resp.clicked() {
        edit_resp.request_focus();
    }
    edit_resp
}

/// 大号蓝色主按钮（连接 / 发送等），占满可用宽。
pub fn primary_button(ui: &mut Ui, label: &str, height: f32) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let bg = if resp.hovered() { colors::BLUE_HOVER } else { colors::BLUE };
    ui.painter()
        .rect(rect, CornerRadius::same(metrics::RADIUS_CTRL as u8), bg, Stroke::NONE, StrokeKind::Inside);
    paint_centered_text(ui, rect, label, FontId::proportional(font::PRIMARY_BTN), colors::WHITE);
    resp
}

/// 次级小按钮（立即重连 / 修改 等）：透明底 + 边框。
pub fn secondary_button(ui: &mut Ui, t: &Theme, label: &str, height: f32) -> Response {
    let pad_x = 12.0;
    let galley = ui
        .painter()
        .layout(label.to_owned(), FontId::proportional(font::AUX), t.text, f32::INFINITY);
    let size = Vec2::new(galley.size().x + pad_x * 2.0, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let bg = if resp.hovered() { t.hover } else { t.input_bg };
    ui.painter().rect(
        rect,
        CornerRadius::same(metrics::RADIUS_CTRL as u8),
        bg,
        Stroke::new(1.0, t.input_border),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        Pos2::new(rect.center().x - galley.size().x / 2.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    resp
}

/// 图标按钮（设置齿轮 / 删除 × / 窗控钮）：透明底，hover 变色；返回响应。
pub fn icon_button(
    ui: &mut Ui,
    t: &Theme,
    icon: Icon,
    size: f32,
    btn: Vec2,
    hover_fill: Option<Color32>,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(btn, Sense::click());
    let (fill, color) = if resp.hovered() {
        (hover_fill.unwrap_or(colors::ROW_HOVER), t.text)
    } else {
        (Color32::TRANSPARENT, t.subtext)
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect(rect, CornerRadius::same(metrics::RADIUS_CTRL as u8), fill, Stroke::NONE, StrokeKind::Inside);
    }
    icons::draw(ui.painter(), rect.center(), size, icon, color);
    resp
}

/// 在 rect 正中画一行文字。
pub fn paint_centered_text(ui: &Ui, rect: Rect, text: &str, font_id: FontId, color: Color32) {
    let galley = ui.painter().layout(text.to_owned(), font_id, color, f32::INFINITY);
    let pos = rect.center() - galley.size() / 2.0;
    ui.painter().galley(Pos2::new(pos.x, pos.y), galley, color);
}

/// 窗口底渐变（Mica 不可用时的 Win10 回退）：3 段色带，色差极小，
/// 用 32 条横带近似径向渐变即可无肉眼色阶。
pub fn paint_window_fallback(ui: &Ui, t: &Theme) {
    let rect = ui.max_rect();
    let [top, mid, bottom] = t.window_bg;
    let bands = 32;
    for i in 0..bands {
        let frac = i as f32 / bands as f32;
        let c = if frac < 0.45 {
            lerp_rgb(top, mid, frac / 0.45)
        } else {
            lerp_rgb(mid, bottom, (frac - 0.45) / 0.55)
        };
        let y0 = rect.top() + rect.height() * frac;
        let y1 = rect.top() + rect.height() * (i + 1) as f32 / bands as f32;
        ui.painter()
            .rect_filled(Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)), CornerRadius::ZERO, c);
    }
}

fn lerp_rgb(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
