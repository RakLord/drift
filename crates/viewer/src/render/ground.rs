//! Background beyond the track: dark ground with a faint grid, so motion
//! reads even when the track is off screen.

use common::Vec2;
use macroquad::prelude::*;

use super::camera::FollowCamera;
use super::{draw_segment, theme};

const SPACING: f64 = 25.0;

pub fn draw(camera: &FollowCamera) {
    clear_background(theme::GROUND);
    let (min, max) = camera.visible();
    let mut x = (min.x / SPACING).floor() * SPACING;
    while x <= max.x {
        draw_segment(Vec2::new(x, min.y), Vec2::new(x, max.y), 0.1, theme::GRID_MINOR);
        x += SPACING;
    }
    let mut y = (min.y / SPACING).floor() * SPACING;
    while y <= max.y {
        draw_segment(Vec2::new(min.x, y), Vec2::new(max.x, y), 0.1, theme::GRID_MINOR);
        y += SPACING;
    }
}
