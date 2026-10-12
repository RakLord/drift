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
}

const BASE_PIXELS_PER_METRE: f64 = 22.0;
const LOOK_AHEAD_SECS: f64 = 0.3;

impl FollowCamera {
    pub fn new(center: Vec2) -> Self {
        Self { center, pixels_per_metre: BASE_PIXELS_PER_METRE, user_zoom: 1.0, view_w: screen_width() }
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

    /// Frame a whole area (map view), immediately.
    pub fn show_area(&mut self, min: Vec2, max: Vec2) {
        self.center = (min + max) / 2.0;
        let size = max - min;
        let fit_x = f64::from(self.view_w) / size.x.max(1.0);
        let fit_y = f64::from(screen_height()) / size.y.max(1.0);
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
        set_camera(&Camera2D {
            target: to_mq(self.center),
            // Negative y zoom makes world +y point up the screen.
            zoom: vec2(2.0 * ppm / self.view_w, -2.0 * ppm / screen_height()),
            viewport: Some((0, 0, self.view_w as i32, screen_height() as i32)),
            ..Default::default()
        });
    }

    /// World-space corners (min, max) of what is on screen.
    pub fn visible(&self) -> (Vec2, Vec2) {
        let half = Vec2::new(f64::from(self.view_w), f64::from(screen_height())) / (2.0 * self.pixels_per_metre);
        (self.center - half, self.center + half)
    }
}
