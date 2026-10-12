//! Drawing. World-space drawing uses metres with +y up (see `camera`);
//! screen-space drawing (HUD, panels) uses pixels with +y down.

pub mod camera;
pub mod car;
pub mod effects;
pub mod ground;
pub mod hud;
pub mod score;
pub mod theme;
pub mod thumb;
pub mod track;

use common::Vec2;
use macroquad::prelude as mq;

pub fn to_mq(v: Vec2) -> mq::Vec2 {
    mq::vec2(v.x as f32, v.y as f32)
}

/// Filled box centred on `center`, `size.x` long along its own x axis and
/// `size.y` wide, rotated by `angle`.
pub fn draw_box(center: Vec2, size: Vec2, angle: f64, color: mq::Color) {
    let hx = Vec2::new(size.x / 2.0, 0.0).rotate(angle);
    let hy = Vec2::new(0.0, size.y / 2.0).rotate(angle);
    let [a, b, c, d] = [center + hx + hy, center + hx - hy, center - hx - hy, center - hx + hy].map(to_mq);
    mq::draw_triangle(a, b, c, color);
    mq::draw_triangle(a, c, d, color);
}

pub fn draw_segment(from: Vec2, to: Vec2, thickness: f64, color: mq::Color) {
    let (a, b) = (to_mq(from), to_mq(to));
    mq::draw_line(a.x, a.y, b.x, b.y, thickness as f32, color);
}

/// Line with a small head, for force and velocity vectors.
pub fn draw_arrow(from: Vec2, to: Vec2, thickness: f64, color: mq::Color) {
    draw_segment(from, to, thickness, color);
    let dir = (to - from).normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    let head = (thickness * 4.0).min((to - from).length() * 0.5);
    let back = to - dir * head;
    let side = dir.perp() * head * 0.5;
    mq::draw_triangle(to_mq(to), to_mq(back + side), to_mq(back - side), color);
}

/// Screen-space text. Returns the text's width in pixels.
pub fn text(s: &str, x: f32, y: f32, size: f32, color: mq::Color) -> f32 {
    mq::draw_text(s, x, y, size, color).width
}
