//! The open test lot: asphalt with a grid, two painted skidpad circles and a
//! slalom row of cones, so speed and rotation are easy to read.

use common::Vec2;
use macroquad::prelude::*;

use super::camera::FollowCamera;
use super::{draw_segment, theme, to_mq};

const MINOR: f64 = 5.0;
const MAJOR: f64 = 25.0;

const SKIDPAD_CENTER: Vec2 = Vec2::new(0.0, 45.0);
const SKIDPAD_RADII: [f64; 2] = [15.0, 30.0];

pub fn draw(camera: &FollowCamera) {
    clear_background(theme::ASPHALT);
    let (min, max) = camera.visible();
    draw_grid(min, max, MINOR, 0.06, theme::GRID_MINOR);
    draw_grid(min, max, MAJOR, 0.12, theme::GRID_MAJOR);

    let c = to_mq(SKIDPAD_CENTER);
    for r in SKIDPAD_RADII {
        draw_circle_lines(c.x, c.y, r as f32, 0.15, theme::PAINT);
    }
    for i in 0..10 {
        draw_cone(Vec2::new(40.0 + f64::from(i) * 15.0, 0.0));
    }
}

fn draw_grid(min: Vec2, max: Vec2, spacing: f64, thickness: f64, color: Color) {
    let mut x = (min.x / spacing).floor() * spacing;
    while x <= max.x {
        draw_segment(Vec2::new(x, min.y), Vec2::new(x, max.y), thickness, color);
        x += spacing;
    }
    let mut y = (min.y / spacing).floor() * spacing;
    while y <= max.y {
        draw_segment(Vec2::new(min.x, y), Vec2::new(max.x, y), thickness, color);
        y += spacing;
    }
}

fn draw_cone(at: Vec2) {
    let p = to_mq(at);
    draw_circle(p.x, p.y, 0.35, theme::CONE);
    draw_circle(p.x, p.y, 0.12, WHITE);
}
