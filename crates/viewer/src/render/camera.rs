use common::Vec2;
use macroquad::prelude::*;

use super::to_mq;

/// Smooth follow camera that looks ahead along the velocity and zooms out
/// with speed. World units are metres, +y up.
pub struct FollowCamera {
    pub center: Vec2,
    pub pixels_per_metre: f64,
    user_zoom: f64,
    /// Width of the screen area the world is drawn in, from the left edge
    /// (the network view can take the rest).
    view_w: f32,
    /// Instead, draw in this screen rectangle (a review grid tile).
    tile: Option<Rect>,
}

const BASE_PIXELS_PER_METRE: f64 = 22.0;
const LOOK_AHEAD_SECS: f64 = 0.3;

impl FollowCamera {
    pub fn new(center: Vec2) -> Self {
        Self { center, pixels_per_metre: BASE_PIXELS_PER_METRE, user_zoom: 1.0, view_w: screen_width(), tile: None }
    }

    pub fn update(&mut self, target: Vec2, velocity: Vec2, dt: f64) {
        let look = target + velocity * LOOK_AHEAD_SECS;
        self.center = self.center.lerp(look, 1.0 - (-dt * 5.0).exp());
        let auto = BASE_PIXELS_PER_METRE / (1.0 + velocity.length() / 35.0);
        let wanted = auto * self.user_zoom;
        self.pixels_per_metre += (wanted - self.pixels_per_metre) * (1.0 - (-dt * 2.0).exp());
    }

    /// Draw the world in the left `width` pixels of the screen.
    pub fn set_view_width(&mut self, width: f32) {
        self.view_w = width.clamp(1.0, screen_width());
    }

    /// Draw the world in this screen rectangle instead.
    pub fn set_tile(&mut self, rect: Rect) {
        self.tile = Some(rect);
    }

    /// The screen rectangle the world is drawn in.
    fn area(&self) -> Rect {
        self.tile.unwrap_or_else(|| Rect::new(0.0, 0.0, self.view_w, screen_height()))
    }

    /// Frame a whole area (map view), immediately.
    pub fn show_area(&mut self, min: Vec2, max: Vec2) {
        self.center = (min + max) / 2.0;
        let size = max - min;
        let area = self.area();
        let fit_x = f64::from(area.w) / size.x.max(1.0);
        let fit_y = f64::from(area.h) / size.y.max(1.0);
        self.pixels_per_metre = fit_x.min(fit_y);
    }

    pub fn snap_to(&mut self, target: Vec2) {
        self.center = target;
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.user_zoom = (self.user_zoom * factor).clamp(0.15, 6.0);
    }

    pub fn apply(&self) {
        let ppm = self.pixels_per_metre as f32;
        let a = self.area();
        set_camera(&Camera2D {
            target: to_mq(self.center),
            // Negative y zoom makes world +y point up the screen.
            zoom: vec2(2.0 * ppm / a.w, -2.0 * ppm / a.h),
            // GL viewports count from the bottom of the window.
            viewport: Some((a.x as i32, (screen_height() - a.y - a.h) as i32, a.w as i32, a.h as i32)),
            ..Default::default()
        });
    }

    /// The world point under screen pixel (x, y).
    pub fn screen_to_world(&self, x: f32, y: f32) -> Vec2 {
        let a = self.area();
        let dx = f64::from(x - a.x - a.w / 2.0);
        let dy = f64::from(a.y + a.h / 2.0 - y);
        self.center + Vec2::new(dx, dy) / self.pixels_per_metre
    }

    /// World-space corners (min, max) of what is on screen.
    pub fn visible(&self) -> (Vec2, Vec2) {
        let a = self.area();
        let half = Vec2::new(f64::from(a.w), f64::from(a.h)) / (2.0 * self.pixels_per_metre);
        (self.center - half, self.center + half)
    }
}
