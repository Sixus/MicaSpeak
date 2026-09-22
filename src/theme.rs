//! MicaSpeak 设计令牌（MD 卡收尾产物）。
//!
//! 全部数值提取自 Figma Make 设计源码（docs/design/src/），人读对照表见 design/tokens.md。
//! MD2a 起各界面模块从这里取色与度量；接线阶段（M1+）不得改动本文件常量，
//! 改设计必须回 Figma 改稿后重新提取。
#![allow(dead_code)] // MD 收尾阶段尚未被任何界面引用

use eframe::egui::{Color32, Vec2};

/// 不分主题的通用色。
pub mod colors {
    use eframe::egui::Color32;

    pub const BLUE: Color32 = Color32::from_rgb(0x00, 0x78, 0xD4);
    pub const BLUE_HOVER: Color32 = Color32::from_rgb(0x1A, 0x86, 0xD9);
    pub const ACCENT2: Color32 = Color32::from_rgb(0x2F, 0xA1, 0xF0); // logo 渐变第二色
    pub const GREEN: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
    pub const YELLOW: Color32 = Color32::from_rgb(0xE8, 0xB2, 0x3A);
    pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
    pub const GRAY: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x8A);
    pub const METER_GREEN: Color32 = Color32::from_rgb(0x46, 0xC4, 0x8B); // 音量条渐变终点
    pub const BLUE_DARK_TEXT: Color32 = Color32::from_rgb(0x5A, 0xB0, 0xFF); // 深色模式下"自己"用
    pub const WHITE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);

    /// 行 hover（CSS 里用 128,128,128,0.14，两主题通用）。
    pub const ROW_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(128, 128, 128, 36);
}

/// 布局度量（px）。
pub mod metrics {
    use eframe::egui::Vec2;

    pub const WIN_MAIN: Vec2 = Vec2::new(420.0, 640.0);
    pub const WIN_MAIN_MIN: Vec2 = Vec2::new(360.0, 520.0);
    pub const WIN_SETTINGS: Vec2 = Vec2::new(520.0, 640.0);
    pub const WIN_OVERLAY: Vec2 = Vec2::new(260.0, 80.0);

    pub const RADIUS_WINDOW: f32 = 8.0;
    pub const RADIUS_CARD: f32 = 8.0;
    pub const RADIUS_CTRL: f32 = 5.0;
    pub const RADIUS_OVERLAY: f32 = 12.0;
    pub const MARGIN: f32 = 12.0; // 卡片距窗口边缘
    pub const GAP: f32 = 8.0;

    pub const CAPTION_H: f32 = 30.0;
    pub const CAPTION_BTN: Vec2 = Vec2::new(40.0, 30.0);

    pub const INPUT_H: f32 = 34.0;
    pub const BTN_PRIMARY_H: f32 = 38.0;
    pub const BTN_SMALL_H: f32 = 26.0;
    pub const PTT_PILL_H: f32 = 28.0;
    pub const COMPOSER_H: f32 = 32.0;

    pub const AVATAR: f32 = 20.0;
    pub const STATUS_DOT: f32 = 8.0;
    pub const UNREAD_DOT: f32 = 6.0;

    pub const TREE_WIDTH_FRAC: f32 = 0.45; // 频道树占主体宽
    pub const USER_ROW_LEFT: f32 = 26.0; // 用户行左缩进（挂在频道 chevron 下）

    pub const SETTINGS_RAIL_W: f32 = 128.0;
    pub const RAIL_INDICATOR: (f32, f32) = (3.0, 16.0); // 选中指示条宽×高

    pub const METER_H: f32 = 4.0; // 底栏音量条
    pub const VAD_BAR_H: f32 = 6.0; // 设置页语音概率条

    pub const RADIO: f32 = 18.0;
    pub const TOGGLE: (f32, f32) = (40.0, 22.0);
    pub const SLIDER_KNOB: f32 = 16.0;

    pub const OVERLAY_HANDLE_SLOT: f32 = 14.0;
    pub const OVERLAY_HANDLE: (f32, f32) = (6.0, 40.0);
}

/// 字号（px）。字重通过 RichText::strong 或独立常量表达。
pub mod font {
    pub const APP_NAME: f32 = 19.0;
    pub const PAGE_TITLE: f32 = 16.0;
    pub const RAIL_TITLE: f32 = 15.0;
    pub const SERVER_NAME: f32 = 13.5;
    pub const BODY: f32 = 13.0;
    pub const CTRL_ROW: f32 = 12.5;
    pub const AUX: f32 = 12.0;
    pub const SMALL: f32 = 11.5;
    pub const SECTION: f32 = 11.0;
    pub const PRIMARY_BTN: f32 = 14.0;
}

/// 一套主题令牌（浅或深）。
pub struct Theme {
    pub dark: bool,
    /// 窗口底渐变（非 Mica 回退用）：[顶部起色, 中段, 底部]
    pub window_bg: [Color32; 3],
    /// 卡片底（86% 不透明）
    pub card: Color32,
    pub card_solid: Color32,
    pub text: Color32,
    pub subtext: Color32,
    pub faint: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    /// Fluent 输入框 2px 加粗下边线
    pub input_bottom: Color32,
    pub hover: Color32,
    pub speaking: Color32,
    pub speaking_border: Color32,
    pub chip: Color32,
    pub scrim: Color32,
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

impl Theme {
    pub const LIGHT: Theme = Theme {
        dark: false,
        window_bg: [
            Color32::from_rgb(0xEE, 0xF2, 0xF8),
            Color32::from_rgb(0xEA, 0xEC, 0xEF),
            Color32::from_rgb(0xF3, 0xF3, 0xF3),
        ],
        card: rgba(255, 255, 255, 219), // 0.86
        card_solid: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        text: Color32::from_rgb(0x1A, 0x1A, 0x1A),
        subtext: rgba(0, 0, 0, 143),             // 0.56
        faint: rgba(0, 0, 0, 97),                // 0.38
        border: rgba(0, 0, 0, 18),               // 0.07
        divider: rgba(0, 0, 0, 15),              // 0.06
        input_bg: rgba(255, 255, 255, 184),      // 0.72
        input_border: rgba(0, 0, 0, 31),         // 0.12
        input_bottom: rgba(0, 0, 0, 102),        // 0.40
        hover: rgba(0, 0, 0, 10),                // 0.04
        speaking: rgba(0, 120, 212, 33),         // 0.13
        speaking_border: rgba(0, 120, 212, 115), // 0.45
        chip: rgba(0, 0, 0, 13),                 // 0.05
        scrim: rgba(240, 240, 240, 89),          // 0.35
    };

    pub const DARK: Theme = Theme {
        dark: true,
        window_bg: [
            Color32::from_rgb(0x2B, 0x2F, 0x3A),
            Color32::from_rgb(0x23, 0x25, 0x2C),
            Color32::from_rgb(0x1C, 0x1C, 0x1F),
        ],
        card: rgba(43, 43, 43, 219),
        card_solid: Color32::from_rgb(0x2B, 0x2B, 0x2B),
        text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        subtext: rgba(255, 255, 255, 158),        // 0.62
        faint: rgba(255, 255, 255, 102),          // 0.40
        border: rgba(255, 255, 255, 23),          // 0.09
        divider: rgba(255, 255, 255, 18),         // 0.07
        input_bg: rgba(255, 255, 255, 15),        // 0.06
        input_border: rgba(255, 255, 255, 31),    // 0.12
        input_bottom: rgba(255, 255, 255, 89),    // 0.35
        hover: rgba(255, 255, 255, 15),           // 0.06
        speaking: rgba(0, 120, 212, 71),          // 0.28
        speaking_border: rgba(64, 164, 255, 140), // 0.55
        chip: rgba(255, 255, 255, 26),            // 0.10
        scrim: rgba(20, 20, 20, 89),              // 0.35
    };

    /// 按当前明暗选套色。
    pub fn pick(dark: bool) -> &'static Theme {
        if dark {
            &Theme::DARK
        } else {
            &Theme::LIGHT
        }
    }

    /// 自己的昵称/麦克风图标用蓝（深色模式换亮蓝以保证对比度）。
    pub fn accent_text(&self) -> Color32 {
        if self.dark {
            colors::BLUE_DARK_TEXT
        } else {
            colors::BLUE
        }
    }
}

/// 窗口最小尺寸便捷取值（eframe ViewportBuilder 用）。
pub fn win_main_min() -> Vec2 {
    metrics::WIN_MAIN_MIN
}
