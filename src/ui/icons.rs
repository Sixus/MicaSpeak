//! Fluent 风格线性图标（1.5px 描边、圆头圆角），几何逐点移植自设计源码
//! docs/design/src/icons.tsx（24×24 视窗）。不依赖系统图标字体，
//! 任何机器上渲染一致。
#![allow(dead_code)] // MD2 阶段控件逐步接入
use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Shape, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Mic,
    Gear,
    Lock,
    ChevronDown,
    ChevronRight,
    Send,
    X,
    Minus,
    Square,
}

/// 在以 `center` 为中心、边长 `size` 的方框内绘制图标。
pub fn draw(painter: &Painter, center: Pos2, size: f32, icon: Icon, color: Color32) {
    let u = size / 24.0;
    let map = |(x, y): (f32, f32)| center + Vec2::new((x - 12.0) * u, (y - 12.0) * u);
    let st = Stroke::new(1.5 * u, color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(Shape::line(pts.iter().map(|&c| map(c)).collect(), st));
    };
    let circle = |c: (f32, f32), r: f32| {
        painter.add(Shape::circle_stroke(map(c), r * u, st));
    };
    let rect = |min: (f32, f32), max: (f32, f32), rr: f32| {
        painter.add(Shape::rect_stroke(
            eframe::egui::Rect::from_two_pos(map(min), map(max)),
            CornerRadius::same((rr * u).round() as u8),
            st,
            StrokeKind::Middle,
        ));
    };
    // 极坐标生成圆弧折线（y 向下，角度顺时针为正；顶部 = -π/2）。
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        let c = map((cx, cy));
        let n = 14;
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                c + Vec2::new(a.cos() * r * u, a.sin() * r * u)
            })
            .collect()
    };

    match icon {
        Icon::Minus => line(&[(5.0, 12.0), (19.0, 12.0)]),
        Icon::Square => rect((6.0, 6.0), (18.0, 18.0), 1.5),
        Icon::X => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::ChevronDown => line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Icon::ChevronRight => line(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]),
        Icon::Lock => {
            rect((5.0, 10.5), (19.0, 20.0), 2.0);
            // 挂钩：左侧上行 → 顶部半圆 → 右侧下行
            let mut pts = vec![map((8.0, 10.5)), map((8.0, 7.5))];
            pts.extend(arc(12.0, 7.5, 4.0, std::f32::consts::PI, std::f32::consts::TAU));
            pts.push(map((16.0, 10.5)));
            painter.add(Shape::line(pts, st));
        }
        Icon::Mic => {
            rect((9.0, 3.0), (15.0, 14.0), 3.0);
            // U 形托弧：从左侧 (6,11) 经底部到右侧 (18,11)
            painter.add(Shape::line(
                arc(12.0, 11.0, 6.0, std::f32::consts::PI, 0.0),
                st,
            ));
            line(&[(12.0, 17.0), (12.0, 21.0)]);
            line(&[(9.0, 21.0), (15.0, 21.0)]);
        }
        Icon::Send => line(&[(4.0, 12.0), (20.0, 4.0), (14.0, 20.0), (11.0, 14.0), (4.0, 12.0)]),
        Icon::Gear => {
            circle((12.0, 12.0), 3.0);
            // 8 根 45° 均布的短齿（r 7.5→9.5），与设计源码 path 一致
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = (a.sin(), a.cos());
                line(&[
                    (12.0 + c * 7.5, 12.0 + s * 7.5),
                    (12.0 + c * 9.5, 12.0 + s * 9.5),
                ]);
            }
        }
    }
}

/// 加载动画（0.9s/圈，随 ctx.time 旋转）。调用方需在动画期间持续请求重绘。
pub fn draw_spinner(painter: &Painter, center: Pos2, size: f32, color: Color32, time_sec: f64) {
    let u = size / 24.0;
    let phase = (time_sec * std::f64::consts::TAU / 0.9) as f32;
    let a0 = -std::f32::consts::FRAC_PI_2 + phase;
    let a1 = a0 - 1.5 * std::f32::consts::PI; // 270° 长弧
    let n = 22;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            center + Vec2::new(a.cos() * 9.0 * u, a.sin() * 9.0 * u)
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new(1.5 * u, color)));
}
