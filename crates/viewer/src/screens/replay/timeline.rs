//! The replay timeline: a bar along the bottom with the playhead, ticks for
//! score events, and click / drag to seek.

use macroquad::prelude::*;
use sim::recording::Recording;
use sim::scoring::ScoreEventKind;

use crate::render::theme;

const HEIGHT: f32 = 10.0;
/// Extra room around the bar that still grabs the mouse.
const GRAB: f32 = 10.0;

pub struct Timeline {
    dragging: bool,
}

impl Timeline {
    pub fn new() -> Self {
        Self { dragging: false }
    }

    /// The bar's area for a view `w` wide.
    fn rect(w: f32) -> Rect {
        Rect::new(16.0, screen_height() - 18.0, (w - 32.0).max(1.0), HEIGHT)
    }

    /// Whether the mouse is on the bar (or dragging it).
    pub fn hovered(&self, w: f32) -> bool {
        let r = Self::rect(w);
        let (mx, my) = mouse_position();
        self.dragging || (mx >= r.x && mx <= r.x + r.w && my >= r.y - GRAB && my <= r.y + r.h + GRAB)
    }

    /// Draw, and return a tick to seek to if the bar was clicked or dragged.
    pub fn show(&mut self, rec: &Recording, tick: f64, w: f32) -> Option<f64> {
        let r = Self::rect(w);
        let ticks = rec.ticks.max(1) as f64;
        let x_of = |t: f64| r.x + r.w * (t / ticks).clamp(0.0, 1.0) as f32;

        draw_rectangle(r.x, r.y, r.w, r.h, theme::PANEL);
        draw_rectangle(r.x, r.y, x_of(tick) - r.x, r.h, Color { a: 0.35, ..theme::NEON_CYAN });
        for e in &rec.events {
            let color = match e.event.kind {
                ScoreEventKind::Banked if e.event.points > 0.0 => theme::GRIP,
                ScoreEventKind::Spin | ScoreEventKind::HardHit => theme::SATURATED,
                ScoreEventKind::Clip { .. } => theme::SODIUM,
                ScoreEventKind::ZoneEnter | ScoreEventKind::Transition | ScoreEventKind::WallTap => theme::NEON_PINK,
                _ => continue,
            };
            draw_line(x_of(e.tick as f64), r.y - 3.0, x_of(e.tick as f64), r.y + r.h + 3.0, 2.0, color);
        }
        let head = x_of(tick);
        draw_rectangle(head - 1.5, r.y - 5.0, 3.0, r.h + 10.0, theme::TEXT);

        if is_mouse_button_pressed(MouseButton::Left) && self.hovered(w) {
            self.dragging = true;
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.dragging = false;
        }
        self.dragging.then(|| {
            let (mx, _) = mouse_position();
            f64::from(((mx - r.x) / r.w).clamp(0.0, 1.0)) * ticks
        })
    }
}
