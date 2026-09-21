//! MicaApp：窗口状态机 + 自绘 Fluent 标题栏 + 演示开关。
//! MD2 界面先行阶段：数据全为假数据，演示开关即未来真功能的接线口。
use crate::theme::{colors, font, metrics, Theme};
use crate::ui::icons::{self as icons, Icon};
use crate::ui::main_window;
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

pub struct Bookmark {
    pub name: String,
    pub addr: String,
}

#[derive(Default)]
pub struct ConnectState {
    pub addr: String,
    pub nick: String,
    pub bookmarks: Vec<Bookmark>,
}

impl ConnectState {
    fn with_demo_data() -> Self {
        ConnectState {
            addr: "voice.kaihei.gg:9987".into(),
            nick: "李四".into(),
            bookmarks: vec![
                Bookmark { name: "开黑联盟".into(), addr: "voice.kaihei.gg:9987".into() },
                Bookmark { name: "深夜电台".into(), addr: "ts.midnight-radio.net".into() },
                Bookmark { name: "设计小组".into(), addr: "10.0.4.21:9987".into() },
            ],
        }
    }
}

/// 连接页交互动作（帧内收集、帧末统一应用）。
#[derive(Clone, Copy, PartialEq)]
enum ConnectAction {
    None,
    Connect,
    ConnectBookmark(usize),
    Delete(usize),
    Add,
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
    pub connect: ConnectState,
    pub main: main_window::MainState,
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
            connect: ConnectState::with_demo_data(),
            main: main_window::MainState::with_demo_data(),
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
        // 底部状态行：灰点 + 未连接
        egui::Panel::bottom("connect_status")
            .exact_size(26.0)
            .frame(egui::Frame::new())
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    w::status_dot(ui, r.center(), w::DotKind::Gray);
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("未连接").size(font::AUX).color(t.subtext));
                });
            });

        // 数据取出为局部变量，避免嵌套闭包多借用 self
        let mut addr = std::mem::take(&mut self.connect.addr);
        let mut nick = std::mem::take(&mut self.connect.nick);
        let bookmarks: Vec<(String, String)> = self
            .connect
            .bookmarks
            .iter()
            .map(|b| (b.name.clone(), b.addr.clone()))
            .collect();
        let connect_error = self.connect_error;
        let mut action = ConnectAction::None;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 6,
                bottom: 8,
            }))
            .show(ui, |ui| {
                // --- 身份与连接卡片 ---
                w::card(t, egui::Margin::same(20)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        paint_logo(ui, rect);
                        ui.add_space(11.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("MicaSpeak")
                                    .size(font::APP_NAME)
                                    .strong()
                                    .color(t.text),
                            );
                            ui.label(
                                egui::RichText::new("轻量 TeamSpeak 客户端")
                                    .size(font::AUX)
                                    .color(t.subtext),
                            );
                        });
                    });
                    ui.add_space(18.0);
                    w::fluent_input(ui, t, "服务器地址", &mut addr, connect_error);
                    ui.add_space(12.0);
                    w::fluent_input(ui, t, "昵称", &mut nick, false);
                    ui.add_space(16.0);
                    if w::primary_button(ui, "连接", metrics::BTN_PRIMARY_H).clicked() {
                        action = ConnectAction::Connect;
                    }
                    if connect_error {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("连接失败：地址无法解析，请检查服务器地址")
                                .size(font::AUX)
                                .color(colors::RED),
                        );
                    }
                });

                ui.add_space(12.0);

                // --- 书签卡片（占满剩余高度）---
                let card_h = ui.available_height();
                w::card(t, egui::Margin { left: 6, right: 6, top: 12, bottom: 8 }).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_height(card_h);
                        ui.label(
                            egui::RichText::new("书签").size(font::SECTION).strong().color(t.faint),
                        );
                        ui.add_space(6.0);
                        let link_h = 30.0;
                        let scroll_h = (ui.available_height() - link_h).max(40.0);
                        ui.allocate_ui(Vec2::new(ui.available_width(), scroll_h), |ui| {
                            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                for (i, (name, baddr)) in bookmarks.iter().enumerate() {
                                    if let Some(a) = bookmark_row(ui, t, i, name, baddr) {
                                        action = a;
                                    }
                                }
                            });
                        });
                        if text_link(ui, t, "＋ 存为书签") {
                            action = ConnectAction::Add;
                        }
                    },
                );
            });

        // 写回状态并应用动作（演示：连接=切主窗口；接线时换成真连接）
        self.connect.addr = addr;
        self.connect.nick = nick;
        match action {
            ConnectAction::None => {}
            ConnectAction::Connect | ConnectAction::ConnectBookmark(_) => {
                if let ConnectAction::ConnectBookmark(i) = action {
                    if let Some(b) = self.connect.bookmarks.get(i) {
                        self.connect.addr = b.addr.clone();
                    }
                }
                self.connect_error = false;
                self.view = View::Main;
            }
            ConnectAction::Delete(i) => {
                self.connect.bookmarks.remove(i);
            }
            ConnectAction::Add => {
                let name = self
                    .connect
                    .addr
                    .split(':')
                    .next()
                    .unwrap_or("服务器")
                    .to_owned();
                self.connect.bookmarks.push(Bookmark { name, addr: self.connect.addr.clone() });
            }
        }
    }

    fn main_page(&mut self, ui: &mut egui::Ui, t: &Theme) {
        // 窗口内按住 Ctrl = PTT 演示（M3 换成全局热键）
        let ctrl_held = ui.input(|i| i.modifiers.ctrl);
        let ptt = self.demo.ptt_held || ctrl_held;
        let speakers = self.demo.speakers.clone();
        let disconnected = self.demo.disconnected;
        let mut gear = false;
        main_window::show(ui, t, &mut self.main, &speakers, ptt, disconnected, &mut gear);
        if gear {
            self.demo.settings_open = true;
        }
    }

    fn placeholder(&mut self, ui: &mut egui::Ui, t: &Theme, title: &str) {
        let dark = self.effective_dark(ui.ctx());
        let speakers = self.demo.speakers.clone();
        let (dis, rec) = (self.demo.disconnected, self.demo.reconnecting);
        let (ov, st) = (self.demo.overlay_open, self.demo.settings_open);
        let (mica, ptt, err) = (self.mica, self.demo.ptt_held, self.connect_error);

        ui.add_space(24.0);
        w::card(t, egui::Margin::same(20)).show(ui, |ui| {
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

// ---------- 连接页部件 ----------

/// 40×40 蓝色圆角 logo 盒 + 白色麦克风图标。
/// 设计稿为 150° 渐变（#0078D4→#2FA1F0），40px 尺寸下纯色观感一致。
fn paint_logo(ui: &Ui, rect: Rect) {
    ui.painter().rect(rect, CornerRadius::same(10), colors::BLUE, Stroke::NONE, StrokeKind::Inside);
    icons::draw(ui.painter(), rect.center(), 20.0, Icon::Mic, colors::WHITE);
}

/// 书签行：名称 + 地址两行、整行 hover、双击连接、右侧 × 删除。
fn bookmark_row(ui: &mut Ui, t: &Theme, idx: usize, name: &str, addr: &str) -> Option<ConnectAction> {
    let h = 46.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
    if resp.hovered() {
        ui.painter().rect(
            rect,
            CornerRadius::same(metrics::RADIUS_CTRL as u8),
            colors::ROW_HOVER,
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
    let name_g = ui
        .painter()
        .layout(name.to_owned(), egui::FontId::proportional(font::BODY), t.text, f32::INFINITY);
    let addr_g = ui
        .painter()
        .layout(addr.to_owned(), egui::FontId::proportional(font::SMALL), t.subtext, f32::INFINITY);
    ui.painter().galley(egui::pos2(rect.left() + 12.0, rect.top() + 6.0), name_g, t.text);
    ui.painter()
        .galley(egui::pos2(rect.left() + 12.0, rect.top() + 25.0), addr_g, t.subtext);

    // 删除 ×（右侧 26×26 热区）
    let x_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 21.0, rect.center().y),
        Vec2::splat(26.0),
    );
    let x_resp = ui.interact(x_rect, ui.id().with("bm_del").with(idx), Sense::click());
    if x_resp.hovered() {
        ui.painter().rect(
            x_rect,
            CornerRadius::same(metrics::RADIUS_CTRL as u8),
            colors::ROW_HOVER,
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
    icons::draw(
        ui.painter(),
        x_rect.center(),
        14.0,
        Icon::X,
        if x_resp.hovered() { t.text } else { t.faint },
    );
    if x_resp.clicked() {
        return Some(ConnectAction::Delete(idx));
    }
    if resp.double_clicked() {
        return Some(ConnectAction::ConnectBookmark(idx));
    }
    None
}

/// 蓝色文字链接按钮（＋ 存为书签 等）。
fn text_link(ui: &mut Ui, _t: &Theme, label: &str) -> bool {
    let galley = ui
        .painter()
        .layout(label.to_owned(), egui::FontId::proportional(font::CTRL_ROW), colors::BLUE, f32::INFINITY);
    let (rect, resp) =
        ui.allocate_exact_size(galley.size() + Vec2::new(8.0, 6.0), Sense::click());
    ui.painter().galley(
        egui::pos2(rect.left() + 4.0, rect.center().y - galley.size().y / 2.0),
        galley,
        colors::BLUE,
    );
    resp.clicked()
}


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
