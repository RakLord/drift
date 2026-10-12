//! What the AI sees and does: all 110 inputs as bars, grouped and labelled,
//! plus its five outputs.

use macroquad::prelude::*;
use sim::Controls;
use sim::sensors::observation::{LAYOUT, Observation, label};

use crate::render::{text, theme};

const ROW: f32 = 13.0;
const BAR: f32 = 70.0;

pub fn draw(observation: &Observation, decision: &Controls) {
    let (x, y) = (16.0, 160.0);
    let width = 2.0 * (190.0 + BAR) + 30.0;
    let rows_per_column = 44;
    draw_rectangle(x, y, width, 40.0 + ROW * (rows_per_column as f32 + 4.0) + 70.0, theme::PANEL_SOLID);
    text("AI INPUTS", x + 10.0, y + 22.0, 20.0, theme::NEON_CYAN);

    // Rays: one tiny column each, 0 (nothing near) to 1 (touching).
    let values = &observation.values;
    let rays = LAYOUT[0].len;
    let (rx, ry) = (x + 10.0, y + 34.0);
    for (i, &v) in values[..rays].iter().enumerate() {
        let h = 26.0 * v.clamp(0.0, 1.0);
        draw_rectangle(rx + i as f32 * 8.0, ry + 26.0 - h, 6.0, h.max(1.0), theme::NEON_PINK);
    }
    text("rays (front left -> right, then rear)", rx, ry + 40.0, 13.0, theme::TEXT_DIM);

    // Everything else: label + centred bar, in two columns.
    let top = ry + 52.0;
    for (k, i) in (rays..values.len()).enumerate() {
        let column = (k / rows_per_column) as f32;
        let row = (k % rows_per_column) as f32;
        let (cx, cy) = (x + 10.0 + column * (200.0 + BAR), top + row * ROW);
        text(&label(i), cx, cy + 10.0, 13.0, theme::TEXT_DIM);
        bar(cx + 120.0, cy + 2.0, values[i]);
    }

    // Outputs.
    let oy = top + ROW * rows_per_column as f32 + 10.0;
    text("OUTPUTS", x + 10.0, oy + 12.0, 16.0, theme::NEON_CYAN);
    let outputs = [
        ("steer", decision.steer),
        ("throttle", decision.throttle),
        ("brake", decision.brake),
        ("handbrake", decision.handbrake),
        ("clutch", decision.clutch),
    ];
    for (k, (name, v)) in outputs.iter().enumerate() {
        let cx = x + 10.0 + (k % 3) as f32 * 160.0;
        let cy = oy + 20.0 + (k / 3) as f32 * ROW * 1.4;
        text(name, cx, cy + 10.0, 13.0, theme::TEXT);
        bar(cx + 70.0, cy + 2.0, *v as f32);
    }
}

/// A bar centred on zero: right/pink for positive, left/cyan for negative.
fn bar(x: f32, y: f32, value: f32) {
    let mid = x + BAR / 2.0;
    draw_rectangle(x, y, BAR, 8.0, theme::GRID_MAJOR);
    let w = value.clamp(-1.0, 1.0) * BAR / 2.0;
    let color = if w >= 0.0 { theme::NEON_PINK } else { theme::NEON_CYAN };
    draw_rectangle(mid.min(mid + w), y, w.abs(), 8.0, color);
    draw_line(mid, y - 1.0, mid, y + 9.0, 1.0, theme::TEXT_DIM);
}
