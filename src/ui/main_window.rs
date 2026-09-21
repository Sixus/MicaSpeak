//! 主窗口（已连接态）：顶栏 / 频道树 / 聊天 / 底部状态栏。
//! MD2 阶段为假数据；M1 接线时数据源换 tsclientlib，渲染层不动。
use crate::theme::{colors, font, metrics, Theme};
use crate::ui::icons::{self as icons, Icon};
use crate::ui::widgets::{self as w, DotKind};
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextFormat, Ui, Vec2};

pub struct User {
    pub name: String,
    pub me: bool,
}

pub struct Group {
    pub name: String,
    pub open: bool,
    pub locked: bool,
    pub users: Vec<User>,
}

pub struct ChatMsg {
    pub time: String,
    pub nick: String,
    pub body: String,
    pub me: bool,
    pub url: bool,
}

#[derive(Default)]
pub struct MainState {
    pub server_name: String,
    pub server_addr: String,
    pub nick: String,
    pub groups: Vec<Group>,
    pub messages: Vec<ChatMsg>,
    pub chat_input: String,
    pub unread_pm: bool,
}

impl MainState {
    pub fn with_demo_data() -> Self {
        MainState {
            server_name: "开黑联盟".into(),
            server_addr: "voice.kaihei.gg".into(),
            nick: "李四".into(),
            groups: vec![
                Group {
                    name: "大厅".into(),
                    open: true,
                    locked: false,
                    users: vec![User { name: "王五".into(), me: false }, User { name: "赵六".into(), me: false }],
                },
                Group {
                    name: "游戏频道".into(),
                    open: true,
                    locked: false,
                    users: vec![
                        User { name: "张三".into(), me: false },
                        User { name: "李四".into(), me: true },
                        User { name: "孙七".into(), me: false },
                    ],
                },
                Group { name: "音乐频道".into(), open: false, locked: true, users: vec![] },
            ],
            messages: vec![
                ChatMsg { time: "14:28".into(), nick: "王五".into(), body: "大家晚上好，今晚开几把？".into(), me: false, url: false },
                ChatMsg { time: "14:30".into(), nick: "张三".into(), body: "房间信息在这".into(), me: false, url: false },
                ChatMsg { time: "14:31".into(), nick: "张三".into(), body: "https://kaihei.gg/room/42".into(), me: false, url: true },
                ChatMsg { time: "14:32".into(), nick: "李四".into(), body: "收到，马上进语音".into(), me: true, url: false },
                ChatMsg { time: "14:33".into(), nick: "孙七".into(), body: "等我五分钟，先热身".into(), me: false, url: false },
            ],
            chat_input: String::new(),
            unread_pm: true,
        }
    }
}

/// 主窗口内容（不含标题栏）。`gear_clicked` 输出齿轮点击。
pub fn show(
    ui: &mut Ui,
    t: &Theme,
    st: &mut MainState,
    speakers: &[&'static str],
    ptt_held: bool,
    countdown: Option<u32>,
    reconnecting: bool,
    gear_clicked: &mut bool,
    reconnect_clicked: &mut bool,
) {
    let full = ui.max_rect();
    let is_disconnected = countdown.is_some();
    // ---- 顶栏 ----
    egui::Panel::top("main_topbar")
        .exact_size(28.0)
        .frame(egui::Frame::new())
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(14.0);
                let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                w::status_dot(
                    ui,
                    r.center(),
                    if is_disconnected { DotKind::Yellow } else { DotKind::Green },
                );
                ui.add_space(9.0);
                ui.label(
                    egui::RichText::new(&st.server_name)
                        .size(13.5)
                        .strong()
                        .color(t.text),
                );
                // 设计稿：顶栏容器 gap 9（点/服务器名/地址/昵称/齿轮等距）
                ui.add_space(9.0);
                ui.label(egui::RichText::new(&st.server_addr).size(font::SMALL).color(t.faint));
                // 弹性空隙
                let addr_end = ui.cursor().right();
                let _ = addr_end;
                ui.allocate_ui_with_layout(
                    Vec2::new((ui.available_width() - 90.0).max(4.0), 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |_| {},
                );
                ui.label(egui::RichText::new(&st.nick).size(font::AUX).color(t.subtext));
                ui.add_space(9.0);
                if w::icon_button(ui, t, Icon::Gear, 17.0, Vec2::splat(28.0), None).clicked() {
                    *gear_clicked = true;
                }
            });
        });

    // ---- 底部状态栏 ----
    egui::Panel::bottom("main_status")
        .exact_size(46.0)
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 6,
        }))
        .show_separator_line(false)
        .show(ui, |ui| {
            bottom_bar(ui, t, ptt_held, countdown, reconnect_clicked);
        });

    // ---- 主体：频道树 | 聊天 ----
    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 2,
            bottom: 8,
        }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let avail = ui.available_width() - metrics::GAP;
                let tree_w = (avail * metrics::TREE_WIDTH_FRAC).round();
                let chat_w = avail - tree_w;
                let h = ui.available_height();
                // 频道树卡片（显式纵向布局，horizontal 内子 ui 会继承横向布局）
                ui.allocate_ui_with_layout(
                    Vec2::new(tree_w, h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        w::card(t, egui::Margin { left: 5, right: 5, top: 8, bottom: 8 }).show(
                            ui,
                            |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_height(ui.available_height());
                                egui::ScrollArea::vertical()
                                    .auto_shrink(false)
                                    .id_salt("tree_scroll")
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        channel_tree(ui, t, &mut st.groups, speakers);
                                    });
                            },
                        );
                    },
                );
                ui.add_space(metrics::GAP);
                // 聊天卡片
                ui.allocate_ui_with_layout(
                    Vec2::new(chat_w, h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        w::card(t, egui::Margin::same(0)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.set_height(ui.available_height());
                            chat(ui, t, st);
                        });
                    },
                );
            });
        });

    // ---- 重连遮罩（断线态最上层）----
    if reconnecting {
        ui.painter().rect_filled(full, CornerRadius::same(0), t.scrim);
        let card_size = Vec2::new(330.0, 60.0);
        let card_rect = Rect::from_center_size(full.center(), card_size);
        ui.painter().rect(
            card_rect,
            CornerRadius::same(metrics::RADIUS_CARD as u8),
            t.card,
            Stroke::new(1.0, t.border),
            StrokeKind::Inside,
        );
        let spinner_c = egui::pos2(card_rect.left() + 24.0, card_rect.center().y);
        icons::draw_spinner(
            ui.painter(),
            spinner_c,
            20.0,
            colors::BLUE,
            ui.ctx().time(),
        );
        let galley = ui.painter().layout(
            "连接已断开，正在尝试恢复…".to_owned(),
            FontId::proportional(font::BODY),
            t.text,
            f32::INFINITY,
        );
        ui.painter().galley(
            Pos2::new(card_rect.left() + 44.0, card_rect.center().y - galley.size().y / 2.0),
            galley,
            t.text,
        );
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
    }
}

// ---------- 频道树 ----------

fn channel_tree(ui: &mut Ui, t: &Theme, groups: &mut [Group], speakers: &[&'static str]) {
    for gi in 0..groups.len() {
        // 频道行
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 29.0),
            Sense::click(),
        );
        if resp.hovered() {
            ui.painter().rect(
                rect,
                CornerRadius::same(metrics::RADIUS_CTRL as u8),
                colors::ROW_HOVER,
                Stroke::NONE,
                StrokeKind::Inside,
            );
        }
        let g = &groups[gi];
        icons::draw(
            ui.painter(),
            egui::pos2(rect.left() + 12.0, rect.center().y),
            15.0,
            if g.open { Icon::ChevronDown } else { Icon::ChevronRight },
            t.faint,
        );
        let name_g = ui
            .painter()
            .layout(g.name.clone(), FontId::proportional(font::BODY), t.text, f32::INFINITY);
        ui.painter().galley(
            egui::pos2(rect.left() + 30.0, rect.center().y - name_g.size().y / 2.0),
            name_g,
            t.text,
        );
        if g.locked {
            icons::draw(
                ui.painter(),
                egui::pos2(rect.right() - 14.0, rect.center().y),
                13.0,
                Icon::Lock,
                t.faint,
            );
        }
        if resp.clicked() {
            groups[gi].open = !groups[gi].open;
        }

        // 成员行（展开时）
        if groups[gi].open {
            for u in &groups[gi].users {
                user_row(ui, t, u, speakers.contains(&u.name.as_str()));
            }
        }
    }
}

fn user_row(ui: &mut Ui, t: &Theme, u: &User, speaking: bool) {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new((ui.available_width() - 8.0).max(20.0), 27.0),
        Sense::hover(),
    );
    let rect = Rect::from_min_size(egui::pos2(rect.left() + 4.0, rect.top()), rect.size());
    if speaking {
        ui.painter().rect(
            rect,
            CornerRadius::same(metrics::RADIUS_CTRL as u8),
            t.speaking,
            Stroke::new(1.0, t.speaking_border),
            StrokeKind::Inside,
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
    // 头像圆
    // 设计稿：头像左缘缩进 26（padding-left），头像 20 宽，昵称再留 8 间隙
    let av_c = egui::pos2(
        rect.left() + metrics::USER_ROW_LEFT + metrics::AVATAR / 2.0,
        rect.center().y,
    );
    ui.painter().circle_filled(av_c, metrics::AVATAR / 2.0, t.chip);
    let ch = u.name.chars().next().map(|c| c.to_string()).unwrap_or_default();
    let ch_g = ui
        .painter()
        .layout(ch, FontId::proportional(10.0), t.subtext, f32::INFINITY);
    ui.painter().galley(
        egui::pos2(av_c.x - ch_g.size().x / 2.0, av_c.y - ch_g.size().y / 2.0),
        ch_g,
        t.subtext,
    );
    // 昵称 + (我)
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &u.name,
        0.0,
        TextFormat::simple(
            FontId::proportional(font::CTRL_ROW),
            if speaking { t.text } else { t.subtext },
        ),
    );
    if u.me {
        job.append(
            " (我)",
            0.0,
            TextFormat::simple(FontId::proportional(font::CTRL_ROW), t.faint),
        );
    }
    let name_g = ui
        .painter()
        .layout_job(job); // TODO: 若无此 API 换 fonts.layout_job
    ui.painter().galley(
        egui::pos2(av_c.x + metrics::AVATAR / 2.0 + 8.0, rect.center().y - name_g.size().y / 2.0),
        name_g,
        t.text,
    );
    if speaking {
        icons::draw(
            ui.painter(),
            egui::pos2(rect.right() - 12.0, rect.center().y),
            13.0,
            Icon::Mic,
            t.accent_text(),
        );
    }
}

// ---------- 聊天 ----------

fn chat(ui: &mut Ui, t: &Theme, st: &mut MainState) {
    // Tab 行（设计稿：容器 padding-left 8、tab 间 gap 2、tab 自身左右 padding 10）
    let (tab_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    ui.painter().line_segment(
        [tab_rect.left_bottom(), tab_rect.right_bottom()],
        Stroke::new(1.0, t.divider),
    );
    let mut x = tab_rect.left() + 8.0;
    x = draw_tab(ui, t, tab_rect, x, "频道", true, false);
    x = draw_tab(ui, t, tab_rect, x, "私聊·张三", false, st.unread_pm);
    let _ = x;

    // 消息列表（设计稿 padding '8px 10px'）
    let msgs_h = ui.available_height() - metrics::COMPOSER_H - 16.0;
    ui.allocate_ui(Vec2::new(ui.available_width(), msgs_h.max(40.0)), |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .id_salt("chat_scroll")
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Frame::new()
                    .inner_margin(egui::Margin { left: 10, right: 10, top: 8, bottom: 8 })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for m in &st.messages {
                            let job = msg_layout(t, m);
                            ui.add(
                                egui::Label::new(job)
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                            ui.add_space(7.0);
                        }
                    });
            });
    });

    // 输入行（设计稿：padding 8 + 输入框 32 → 整行高 48）
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), metrics::COMPOSER_H + 16.0),
        Sense::hover(),
    );
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, t.divider),
    );
    let input_rect = Rect::from_min_max(
        rect.left_top() + Vec2::new(8.0, 8.0),
        rect.right_top() + Vec2::new(-8.0 - metrics::COMPOSER_H - 6.0, 8.0 + metrics::COMPOSER_H),
    );
    ui.painter().rect(
        input_rect,
        CornerRadius::same(metrics::RADIUS_CTRL as u8),
        t.input_bg,
        Stroke::new(1.0, t.input_border),
        StrokeKind::Inside,
    );
    let edit = egui::TextEdit::singleline(&mut st.chat_input)
        .frame(egui::Frame::new())
        .desired_width(input_rect.width() - 20.0)
        .hint_text("发送消息到 频道…")
        .font(FontId::proportional(font::CTRL_ROW))
        .text_color(t.text)
        .vertical_align(egui::Align::Center);
    let edit_rect = Rect::from_min_size(
        egui::pos2(input_rect.left() + 10.0, input_rect.center().y - 11.0),
        Vec2::new(input_rect.width() - 20.0, 22.0),
    );
    let edit_resp = ui.put(edit_rect, edit);
    let mut send_now = false;
    if edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        send_now = true;
        edit_resp.request_focus();
    }
    // 发送按钮
    let send_rect = Rect::from_min_size(
        egui::pos2(rect.right() - 8.0 - metrics::COMPOSER_H, rect.top() + 8.0),
        Vec2::splat(metrics::COMPOSER_H),
    );
    let send_resp = ui.interact(send_rect, ui.id().with("chat_send"), Sense::click());
    let send_bg = if send_resp.hovered() { colors::BLUE_HOVER } else { colors::BLUE };
    ui.painter().rect(
        send_rect,
        CornerRadius::same(metrics::RADIUS_CTRL as u8),
        send_bg,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    icons::draw(ui.painter(), send_rect.center(), 15.0, Icon::Send, colors::WHITE);
    if send_resp.clicked() {
        send_now = true;
    }
    if send_now && !st.chat_input.trim().is_empty() {
        st.messages.push(ChatMsg {
            time: now_hhmm(),
            nick: st.nick.clone(),
            body: st.chat_input.trim().to_owned(),
            me: true,
            url: false,
        });
        st.chat_input.clear();
    }
}

/// 当前时刻 hh:mm（演示用；不做时区处理）。
fn now_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let day = secs % 86_400;
    format!("{:02}:{:02}", day / 3600 + 8, (day % 3600) / 60)
}

/// 画一个 Tab，返回下一个 Tab 的起始 x。
/// 设计稿（MainFrame.tsx）：tab 自身 padding '6px 10px 8px'、tab 间 gap 2、
/// 未读点距文字 5、激活态 2px 蓝色下划线横跨 tab 全宽（含左右 padding）。
fn draw_tab(
    ui: &Ui,
    t: &Theme,
    tab_rect: Rect,
    x_in: f32,
    label: &str,
    active: bool,
    unread: bool,
) -> f32 {
    let galley = ui.painter().layout(
        label.to_owned(),
        FontId::proportional(font::CTRL_ROW),
        if active { t.text } else { t.subtext },
        f32::INFINITY,
    );
    let (gw, gh) = (galley.size().x, galley.size().y);
    let text_x = x_in + 10.0; // tab 左内边距
    ui.painter().galley(
        egui::pos2(text_x, tab_rect.center().y - gh / 2.0),
        galley,
        if active { t.text } else { t.subtext },
    );
    if active {
        // 激活 Tab 的 2px 蓝色下划线（压在分隔线上）
        let y = tab_rect.bottom() - 0.5;
        ui.painter().line_segment(
            [egui::pos2(text_x - 10.0, y), egui::pos2(text_x + gw + 10.0, y)],
            Stroke::new(2.0, colors::BLUE),
        );
    }
    let mut x = text_x + gw;
    if unread {
        x += 5.0;
        ui.painter()
            .circle_filled(egui::pos2(x + 3.0, tab_rect.center().y), metrics::UNREAD_DOT / 2.0, colors::RED);
        x += metrics::UNREAD_DOT;
    }
    x + 10.0 + 2.0 // tab 右内边距 + tab 间 gap
}

fn msg_layout(t: &Theme, m: &ChatMsg) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &m.time,
        6.0,
        TextFormat::simple(FontId::proportional(font::CTRL_ROW), t.faint),
    );
    let nick_color = if m.me { t.accent_text() } else { t.text };
    let nick = if m.me { format!("{} (我)", m.nick) } else { m.nick.clone() };
    job.append(
        &nick,
        6.0,
        TextFormat::simple(FontId::proportional(font::CTRL_ROW), nick_color)
            .into(),
    );
    let body_fmt = if m.url {
        let mut f = TextFormat::simple(FontId::proportional(font::CTRL_ROW), colors::BLUE);
        f.underline = Stroke::new(1.0, colors::BLUE);
        f
    } else {
        TextFormat::simple(FontId::proportional(font::CTRL_ROW), t.text)
    };
    // 设计稿：昵称 marginRight 6 → 正文前留 6px
    job.append(&m.body, 6.0, body_fmt.into());
    job
}

// ---------- 底部状态栏 ----------

fn bottom_bar(
    ui: &mut Ui,
    t: &Theme,
    ptt_held: bool,
    countdown: Option<u32>,
    reconnect_clicked: &mut bool,
) {
    ui.horizontal_centered(|ui| {
        if let Some(secs) = countdown {
            ui.label(
                egui::RichText::new(format!("已断开 · {secs} 秒后自动重连"))
                    .size(font::AUX)
                    .color(t.subtext),
            );
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width().max(4.0) - 90.0, 26.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if w::secondary_button(ui, t, "立即重连", metrics::BTN_SMALL_H).clicked() {
                        *reconnect_clicked = true;
                    }
                },
            );
            return;
        }
        // PTT 胶囊（按住高亮；演示态由 demo.ptt_held 驱动）
        let label = "左Ctrl 按住说话";
        let galley = ui.painter().layout(
            label.to_owned(),
            FontId::proportional(font::AUX),
            if ptt_held { colors::WHITE } else { t.subtext },
            f32::INFINITY,
        );
        let pill = Vec2::new(galley.size().x + 14.0 + 20.0, metrics::PTT_PILL_H);
        let (rect, resp) = ui.allocate_exact_size(pill, Sense::click());
        let (bg, border) = if ptt_held {
            (colors::BLUE, Stroke::NONE)
        } else {
            (t.input_bg, Stroke::new(1.0, t.input_border))
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(metrics::RADIUS_CTRL as u8),
            bg,
            border,
            StrokeKind::Inside,
        );
        icons::draw(
            ui.painter(),
            egui::pos2(rect.left() + 12.0, rect.center().y),
            14.0,
            Icon::Mic,
            if ptt_held { colors::WHITE } else { t.subtext },
        );
        ui.painter().galley(
            egui::pos2(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0),
            galley,
            if ptt_held { colors::WHITE } else { t.subtext },
        );
        let _ = resp;

        ui.add_space(12.0);

        // 音量条：4px 轨道 + 蓝→绿渐变填充（演示：PTT 按住时动画）
        let meter = Vec2::new(ui.available_width() - 80.0, metrics::METER_H);
        let (rect, _) = ui.allocate_exact_size(meter, Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(metrics::METER_H as u8 / 2),
            t.chip,
            Stroke::NONE,
            StrokeKind::Inside,
        );
        if ptt_held {
            let time = ui.ctx().time() as f32;
            let level = 0.60 + 0.12 * (time * 2.2).sin();
            let fill_w = rect.width() * level;
            // 4 条竖带近似蓝→绿水平渐变
            let n = 4;
            for i in 0..n {
                let x0 = rect.left() + fill_w * i as f32 / n as f32;
                let x1 = rect.left() + fill_w * (i + 1) as f32 / n as f32;
                let c = lerp_color(colors::BLUE, colors::METER_GREEN, (i as f32 + 0.5) / n as f32);
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x0.min(rect.right()), rect.top()),
                        egui::pos2(x1.min(rect.right()), rect.bottom()),
                    ),
                    CornerRadius::same(0),
                    c,
                );
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }

        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("延迟 32ms").size(font::AUX).color(t.subtext),
        );
    });
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
