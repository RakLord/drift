//! The output side: each output's name, value bar and a sparkline of the
//! last few seconds.

use std::collections::VecDeque;

use macroquad::prelude::*;
use nn::Activation;

use super::health::WINDOW;
use crate::render::{text, theme};

pub const NAMES: [&str; 5] = ["steer", "throttle", "brake", "handbrake", "clutch"];

pub fn name(index: usize) -> String {
    NAMES.get(index).map_or_else(|| format!("out {index}"), |n| (*n).to_owned())
}

/// Whether values run -1..1 (true) or 0..1.
fn signed(activation: Activation) -> bool {
    activation != Activation::Sigmoid
}

#[derive(Default)]
pub struct OutputHistory {
    values: Vec<VecDeque<f32>>,
}

impl OutputHistory {
    pub fn clear(&mut self) {
        self.values.clear();
    }

    pub fn push(&mut self, outputs: &[f32]) {
        self.values.resize_with(outputs.len(), || VecDeque::with_capacity(WINDOW));
        for (history, &v) in self.values.iter_mut().zip(outputs) {
            if history.len() == WINDOW {
                history.pop_front();
            }
            history.push_back(v);
        }
    }

    /// Name, value bar and sparkline in the `width` pixels right of output
    /// `index`'s node.
    pub fn draw(&self, index: usize, at: Vec2, radius: f32, width: f32, value: f32, activation: Activation) {
        let signed = signed(activation);
        let x = at.x + radius + 10.0;
        text(&name(index), x, at.y - 5.0, 16.0, theme::TEXT);
        text(&format!("{value:+.2}"), x + 74.0, at.y + 9.0, 13.0, theme::TEXT_DIM);

        // Value bar: centred for steer, from the left for pedals.
        let (bw, by) = (70.0, at.y + 2.0);
        draw_rectangle(x, by, bw, 6.0, theme::GRID_MAJOR);
        let color = if value >= 0.0 { theme::NEON_PINK } else { theme::NEON_CYAN };
        if signed {
            let mid = x + bw / 2.0;
            let w = value.clamp(-1.0, 1.0) * bw / 2.0;
            draw_rectangle(mid.min(mid + w), by, w.abs(), 6.0, color);
            draw_line(mid, by - 1.0, mid, by + 7.0, 1.0, theme::TEXT_DIM);
        } else {
            draw_rectangle(x, by, value.clamp(0.0, 1.0) * bw, 6.0, color);
        }

        // Sparkline, oldest on the left.
        let Some(history) = self.values.get(index) else { return };
        let sx = x + 112.0;
        let sw = at.x + width - 8.0 - sx;
        if sw < 30.0 {
            return;
        }
        let (sy, sh) = (at.y - 16.0, 30.0);
        draw_rectangle(sx, sy, sw, sh, theme::PANEL);
        let to_y = |v: f32| {
            let t = if signed { (v.clamp(-1.0, 1.0) + 1.0) / 2.0 } else { v.clamp(0.0, 1.0) };
            sy + sh * (1.0 - t)
        };
        if signed {
            draw_line(sx, sy + sh / 2.0, sx + sw, sy + sh / 2.0, 1.0, theme::GRID_MAJOR);
        }
        let step = sw / (WINDOW - 1) as f32;
        let start = WINDOW - history.len();
        for (j, (a, b)) in history.iter().zip(history.iter().skip(1)).enumerate() {
            let x0 = sx + (start + j) as f32 * step;
            draw_line(x0, to_y(*a), x0 + step, to_y(*b), 1.5, theme::SODIUM);
        }
    }
}
