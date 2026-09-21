//! MicaApp：窗口状态机 + 自绘 Fluent 标题栏 + 演示开关。
//! MD2 界面先行阶段：数据全为假数据，演示开关即未来真功能的接线口。
use crate::theme::{colors, font, metrics, Theme};
use crate::ui::icons::{self as icons, Icon};
use crate::ui::widgets::{self as w};
use eframe::egui::{self, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// 连接页（未连接）
    Connect,
    /// 主窗口（已连接）
    Main,
}

/// MD2 演示开关驱动的状态汇总。
#[derive(Default)]
pub struct Demo {
    /// 说话人演示：空 = 无人；1 人 = 张三；2 人 = 说话态（PTT 按下）
    pub speakers: Vec<&'static str>,
    pub ptt_held: bool,
    pub disconnected: bool,
    pub reconnecting: bool,
    pub overlay_open: bool,
    pub settings_open: bool,
}

pub struct MicaApp {
    /// Mica 是否成功应用（false = Win10 回退渐变底）
    pub mica: bool,
    pub view: View,
    /// 连接页表单错误演示（红框 + 红字）
    pub connect_error: bool,
    /// None = 跟随系统（默认），演示开关可覆盖
    theme_pref: Option<egui::ThemePreference>,
    pub demo: Demo,
    mica_dark: Option<bool>,
}

impl MicaApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let dark = cc.egui_ctx.theme() == egui::Theme::Dark;
        let mica = window_vibrancy::apply_mica(cc, Some(dark)).is_ok();
        let mut app = MicaApp {
            mica,
            view: View::Connect,
            connect_error: false,
            theme_pref: None,
            demo: Demo::default(),
            mica_dark: if mica { Some(dark) } else { None },
        };
        app.apply_env_state(&cc.egui_ctx);
        app
    }

    /// MICASPEAK_UI_STATE 环境变量：截图自检 / 演示固定状态用。
    fn apply_env_state(&mut self, ctx: &egui::Context) {
        let pref = |p: egui::ThemePreference| Some(p);
        match std::env::var("MICASPEAK_UI_STATE").unwrap_or_default().as_str() {
            "main" => self.view = View::Main,
            "main-dark" => {
                self.view = View::Main;
                self.theme_pref = pref(egui::ThemePreference::Dark);
            }
            "speaking" => {
                self.view = View::Main;
                self.demo.speakers = vec!["张三", "李四"];
            }
            "ptt" => {
                self.view = View::Main;
                self.demo.ptt_held = true;
                self.demo.speakers = vec!["张三", "李四"];
            }
            "disconnected" => {
                self.view = View::Main;
                self.demo.disconnected = true;
                self.demo.reconnecting = true;
            }
            "settings" => {
                self.view = View::Main;
                self.demo.settings_open = true;
            }
            "overlay" => {
                self.view = View::Main;
                self.demo.overlay_open = true;
            }
            "error" => {
                self.view = View::Connect;
                self.connect_error = true;
            }
            _ => {}
        }
        if let Some(p) = self.theme_pref {
            ctx.set_theme(p);
        }
    }

    fn effective_dark(&self, ctx: &egui::Context) -> bool {
        ctx.theme() == egui::Theme::Dark
    }

    fn cycle_theme(&mut self, ctx: &egui::Context) {
        let next = match self.theme_pref {
            None => egui::ThemePreference::Light,
            Some(egui::ThemePreference::Light) => egui::ThemePreference::Dark,
            _ => egui::ThemePreference::System,
        };
        self.theme_pref = if next == egui::ThemePreference::System {
            None
        } else {
            Some(next)
        };
        ctx.set_theme(next);
    }

    fn cycle_speaking(&mut self) {
        self.demo.speakers = match self.demo.speakers.len() {
            0 => vec!["张三"],
            1 => vec!["张三", "李四"],
            _ => Vec::new(),
        };
    }

    // ---------- 窗口骨架 ----------

    fn caption_bar(&mut self, ui: &mut egui::Ui, t: &Theme) {
        let theme_label = match self.theme_pref {
            None => "主题·跟随",
            Some(egui::ThemePreference::Light) => "主题·浅",
            _ => "主题·深",
        };
        let mut cycle_theme = false;
        let mut cycle_speaking = false;
        let mut toggle_disconnect = false;
        let mut overlay_open = self.demo.overlay_open;
        let mut settings_open = self.demo.settings_open;

        egui::Panel::top("caption")
            .exact_size(metrics::CAPTION_H)
            .frame(egui::Frame::new())
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if demo_pill(ui, t, theme_label, false).clicked() {
                        cycle_theme = true;
                    }
                    if demo_pill(ui, t, "悬浮", overlay_open).clicked() {
                        overlay_open = !overlay_open;
                    }
                    if demo_pill(ui, t, "断线", self.demo.disconnected).clicked() {
                        toggle_disconnect = true;
                    }
                    if demo_pill(ui, t, "说话", !self.demo.speakers.is_empty()).clicked() {
                        cycle_speaking = true;
                    }
                    if demo_pill(ui, t, "设置窗", settings_open).clicked() {
                        settings_open = !settings_open;
                    }
                    // 标题栏空白区（右侧给窗控钮组留位，含按钮间距）：拖动移动窗口 / 双击最大化
                    let buttons_w = 3.0 * metrics::CAPTION_BTN.x + 2.0 * 8.0;
                    let full = ui.available_rect_before_wrap();
                    let drag_rect = egui::Rect::from_min_max(
                        full.min,
                        egui::pos2(full.right() - buttons_w, full.bottom()),
                    );
                    let drag = ui.allocate_rect(drag_rect, Sense::drag());
                    if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    caption_buttons(ui, t);
                });
            });

        if cycle_theme {
            self.cycle_theme(ui.ctx());
        }
        if cycle_speaking {
            self.cycle_speaking();
        }
        if toggle_disconnect {
            self.demo.disconnected = !self.demo.disconnected;
            self.demo.reconnecting = self.demo.disconnected;
        }
        self.demo.overlay_open = overlay_open;
        self.demo.settings_open = settings_open;
    }

    fn body(&mut self, ui: &mut egui::Ui, t: &Theme) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
            .show(ui, |ui| {
                match self.view {
                    View::Connect => self.connect_page(ui, t),
                    View::Main => self.main_page(ui, t),
                }
            });
    }

    // ---------- 各屏（MD2b/MD2c 替换为正式实现） ----------

    fn connect_page(&mut self, ui: &mut egui::Ui, t: &Theme) {
        self.placeholder(ui, t, "01-连接页（MD2b 实现）");
    }

    fn main_page(&mut self, ui: &mut egui::Ui, t: &Theme) {
        self.placeholder(ui, t, "02-主窗口（MD2c 实现）");
    }

    fn placeholder(&mut self, ui: &mut egui::Ui, t: &Theme, title: &str) {
        let dark = self.effective_dark(ui.ctx());
        let speakers = self.demo.speakers.clone();
        let (dis, rec) = (self.demo.disconnected, self.demo.reconnecting);
        let (ov, st) = (self.demo.overlay_open, self.demo.settings_open);
        let (mica, ptt, err) = (self.mica, self.demo.ptt_held, self.connect_error);

        ui.add_space(24.0);
        w::card(t, 20).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).size(16.0).strong().color(t.text));
            ui.add_space(8.0);
            for (k, v) in [
                ("Mica", mica.to_string()),
                ("主题", if dark { "深" } else { "浅" }.to_string()),
                ("说话人", speakers.join("、")),
                ("PTT", ptt.to_string()),
                ("断线/重连中", format!("{dis}/{rec}")),
                ("悬浮窗/设置窗", format!("{ov}/{st}")),
                ("表单错误", err.to_string()),
            ] {
                ui.label(
                    egui::RichText::new(format!("{k}: {v}"))
                        .size(font::AUX)
                        .color(t.subtext),
                );
            }
        });
    }
}

impl eframe::App for MicaApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // 主题切换后重刷 Mica 明暗（apply_mica 可重复调用）
        let dark = self.effective_dark(ui.ctx());
        if self.mica && self.mica_dark != Some(dark) {
            if window_vibrancy::apply_mica(frame, Some(dark)).is_ok() {
                self.mica_dark = Some(dark);
            }
        }
        let t = Theme::pick(dark);
        // 窗口底：设计稿令牌渐变（React 稿本身即用渐变模拟 Mica）。
        // 真 Mica 依赖 swapchain 透明，wgpu/DX12 下 alpha 合成不可用（渲染为黑），
        // 故 MD2 阶段始终自绘渐变；真材质留 M5 攻关（glow/Vulkan 后端）。
        w::paint_window_fallback(ui, t);
        self.caption_bar(ui, t);
        self.body(ui, t);
    }
}

// ---------- 标题栏部件 ----------

/// 演示开关小胶囊（MD2 阶段专用，接线时整体移除）。
fn demo_pill(ui: &mut Ui, t: &Theme, label: &str, active: bool) -> Response {
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(font::SECTION),
        if active { colors::WHITE } else { t.subtext },
        f32::INFINITY,
    );
    let size = Vec2::new(galley.size().x + 16.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if active {
        colors::BLUE
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect(rect, CornerRadius::same(4), fill, Stroke::NONE, StrokeKind::Inside);
    }
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        Color32::TRANSPARENT,
        Stroke::new(1.0, t.input_border),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        if active { colors::WHITE } else { t.subtext },
    );
    resp
}

/// 右上角窗控钮组：最小化 / 最大化 / 关闭（关闭 hover 红）。
fn caption_buttons(ui: &mut Ui, t: &Theme) {
    let btn = metrics::CAPTION_BTN;
    // 最小化
    let (rect, resp) = ui.allocate_exact_size(btn, Sense::click());
    paint_caption_button(ui, t, rect, &resp, Icon::Minus, 14.0, false);
    if resp.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    // 最大化
    let (rect, resp) = ui.allocate_exact_size(btn, Sense::click());
    paint_caption_button(ui, t, rect, &resp, Icon::Square, 12.0, false);
    if resp.clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    // 关闭
    let (rect, resp) = ui.allocate_exact_size(btn, Sense::click());
    paint_caption_button(ui, t, rect, &resp, Icon::X, 14.0, true);
    if resp.clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn paint_caption_button(
    ui: &Ui,
    t: &Theme,
    rect: Rect,
    resp: &Response,
    icon: Icon,
    icon_size: f32,
    is_close: bool,
) {
    if resp.hovered() {
        let fill = if is_close { colors::RED } else { colors::ROW_HOVER };
        ui.painter().rect(rect, CornerRadius::ZERO, fill, Stroke::NONE, StrokeKind::Inside);
    }
    let color = if is_close && resp.hovered() {
        colors::WHITE
    } else {
        t.subtext
    };
    icons::draw(ui.painter(), rect.center(), icon_size, icon, color);
}
