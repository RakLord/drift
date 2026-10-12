//! Rolling graphs of a few signals over the last few seconds.

use std::collections::VecDeque;

use macroquad::prelude::*;
use sim::car::CarState;
use sim::car::wheel::RL;

use crate::render::{text, theme};

const SAMPLES: usize = 600; // 5 s at 120 Hz

struct Series {
    label: &'static str,
    color: Color,
    /// Value that maps to the top/bottom edge of the graph.
    range: f64,
    values: VecDeque<f64>,
}

pub struct History {
    series: Vec<Series>,
}

impl History {
    pub fn new() -> Self {
        let series = |label, color, range| Series { label, color, range, values: VecDeque::with_capacity(SAMPLES) };
        Self {
            series: vec![
                series("drift angle (±60°)", theme::NEON_PINK, 60.0),
                series("steer (±60°)", theme::NEON_CYAN, 60.0),
                series("rear slip ratio (±2)", theme::SODIUM, 2.0),
            ],
        }
    }

    /// Record one sim tick.
    pub fn record(&mut self, state: &CarState) {
        let values = [
            state.body_slip_angle().to_degrees(),
            state.steer_angle.to_degrees(),
            state.wheels[RL].slip_ratio,
        ];
        for (series, value) in self.series.iter_mut().zip(values) {
            if series.values.len() == SAMPLES {
                series.values.pop_front();
            }
            series.values.push_back(value);
        }
    }

    pub fn clear(&mut self) {
        for series in &mut self.series {
            series.values.clear();
        }
    }

    pub fn draw(&self) {
        let (w, h) = (420.0, 140.0);
        let (x, y) = (16.0, 16.0);
        draw_rectangle(x, y, w, h, theme::PANEL);
        let mid = y + h / 2.0;
        draw_line(x, mid, x + w, mid, 1.0, theme::GRID_MAJOR);
        for (i, series) in self.series.iter().enumerate() {
            let to_point = |(j, v): (usize, &f64)| {
                let px = x + w * j as f32 / SAMPLES as f32;
                let py = mid - (v / series.range).clamp(-1.0, 1.0) as f32 * (h / 2.0 - 4.0);
                vec2(px, py)
            };
            let points: Vec<Vec2> = series.values.iter().enumerate().map(to_point).collect();
            for pair in points.windows(2) {
                draw_line(pair[0].x, pair[0].y, pair[1].x, pair[1].y, 1.5, series.color);
            }
            text(series.label, x + 8.0, y + h + 18.0 + i as f32 * 18.0, 16.0, series.color);
        }
    }
}
