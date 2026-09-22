//! 悬浮窗（独立置顶 viewport，260×80 深色半透明胶囊）。
//! 永远深色，独立于主题；左侧 6px 把手可拖动整窗。
//! MD2 演示说话人来自 demo；M4 接线时换成真实说话状态。
//! 注：真半透明依赖 swapchain alpha（wgpu/DX12 下不可用，回退为不透明深底），
//! M5 材质卡攻关（glow 后端或 DWM 缩略图方案）。
use crate::theme::{colors, font, metrics};
use crate::ui::icons::{self as icons, Icon};
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, StrokeKind, Ui, Vec2};

// DX12/wgpu does not composite premultiplied alpha for transparent viewports
// reliably. Use the prototype's near-black surface as an opaque fallback so
// the independent viewport never exposes a black rectangle around the pill.
pub const PILL: Color32 = Color32::from_rgb(24, 24, 27);
pub const HANDLE: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 89); // 0.35
pub const EMPTY_TEXT: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 140); // 0.55

/// 悬浮窗内容（viewport 回调内）。
pub fn show(ui: &mut Ui, speakers: &[String]) {
    let rect = ui.max_rect();

    // 胶囊底：整窗即胶囊（12px 圆角深底）
    ui.painter().rect(
        rect,
        CornerRadius::same(metrics::RADIUS_OVERLAY as u8),
        PILL,
        egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
        StrokeKind::Inside,
    );

    // 把手槽（14px 宽）+ 6×40 竖把手；拖动整窗
    let handle_rect = Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + metrics::OVERLAY_HANDLE_SLOT, rect.bottom()),
    );
    let resp = ui.allocate_rect(handle_rect, Sense::drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let (hw, hh) = metrics::OVERLAY_HANDLE;
    ui.painter().rect_filled(
        Rect::from_center_size(
            egui::pos2(
                rect.left() + metrics::OVERLAY_HANDLE_SLOT / 2.0,
                rect.center().y,
            ),
            Vec2::new(hw, hh),
        ),
        CornerRadius::same(3),
        HANDLE,
    );

    // 内容区（把手右侧）
    let content = Rect::from_min_max(
        egui::pos2(
            rect.left() + metrics::OVERLAY_HANDLE_SLOT + 2.0,
            rect.top() + 12.0,
        ),
        egui::pos2(rect.right() - 14.0, rect.bottom() - 12.0),
    );
    if speakers.is_empty() {
        let galley = ui.painter().layout(
            "等待说话…".to_owned(),
            FontId::proportional(font::CTRL_ROW),
            EMPTY_TEXT,
            f32::INFINITY,
        );
        ui.painter().galley(
            Pos2::new(content.left(), content.center().y - galley.size().y / 2.0),
            galley,
            EMPTY_TEXT,
        );
    } else {
        let row_h = (content.height() / speakers.len() as f32).min(28.0);
        for (i, name) in speakers.iter().enumerate() {
            let cy = content.top() + row_h * i as f32 + row_h / 2.0;
            // 麦克风圆标（20px 蓝底白图标）
            let c = egui::pos2(content.left() + 10.0, cy);
            ui.painter().circle_filled(
                c,
                10.0,
                Color32::from_rgba_unmultiplied_const(0, 120, 212, 230),
            ); // 0.9
            icons::draw(ui.painter(), c, 12.0, Icon::Mic, colors::WHITE);
            let galley = ui.painter().layout(
                name.clone(),
                FontId::proportional(13.5),
                colors::WHITE,
                f32::INFINITY,
            );
            ui.painter().galley(
                Pos2::new(content.left() + 26.0, cy - galley.size().y / 2.0),
                galley,
                colors::WHITE,
            );
        }
    }
}
