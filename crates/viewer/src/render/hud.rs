//! Driver HUD (screen space): speed, tach, drift angle, input bars.

use macroquad::prelude::*;
use sim::{Controls, World};
use sim::car::{CarParams, CarState};

use super::{text, theme};

pub fn draw(state: &CarState, params: &CarParams) {
    let h = screen_height();
    let x = 24.0;

    let kmh = state.speed() * 3.6;
    text(&format!("{kmh:.0}"), x, h - 104.0, 64.0, theme::TEXT);
    text("km/h", x + 4.0, h - 82.0, 20.0, theme::TEXT_DIM);

    let drift = state.body_slip_angle().to_degrees().abs();
    let drift_color = if drift > 10.0 { theme::NEON_PINK } else { theme::TEXT_DIM };
    text(&format!("{drift:.0}°"), x + 150.0, h - 104.0, 48.0, drift_color);
    text("angle", x + 152.0, h - 82.0, 20.0, theme::TEXT_DIM);

    draw_tach(state, params, x, h - 66.0, 300.0);
    draw_inputs(&state.controls, screen_width() - 240.0, h - 160.0);
}

/// Track name and position along the lap, top centre.
pub fn draw_track_info(world: &World) {
    let track = &world.track;
    let lap = (world.progress / track.length).floor().max(0.0) as u32 + 1;
    let line = format!(
        "{}   lap {lap}   {:.0} / {:.0} m",
        track.def.name, world.place.distance, track.length
    );
    text(&line, screen_width() / 2.0 - 140.0, 20.0, 18.0, theme::TEXT_DIM);
}

fn draw_tach(state: &CarState, params: &CarParams, x: f32, y: f32, width: f32) {
    let limit = params.engine.rev_limit_rpm;
    let fraction = (state.engine_rpm() / limit).clamp(0.0, 1.0) as f32;
    draw_rectangle(x, y, width, 10.0, theme::PANEL);
    let color = if fraction > 0.92 { theme::SATURATED } else { theme::SODIUM };
    draw_rectangle(x, y, width * fraction, 10.0, color);
    let label = format!("{:.0} rpm", state.engine_rpm());
    text(&label, x + width + 10.0, y + 10.0, 20.0, theme::TEXT_DIM);
}

fn draw_inputs(c: &Controls, x: f32, y: f32) {
    let rows = [
        ("thr", c.throttle, theme::GRIP),
        ("brk", c.brake, theme::SATURATED),
        ("hbk", c.handbrake, theme::SODIUM),
        ("clu", c.clutch, theme::NEON_CYAN),
    ];
    let width = 160.0;
    for (i, (label, value, color)) in rows.into_iter().enumerate() {
        let row_y = y + i as f32 * 20.0;
        text(label, x, row_y + 11.0, 18.0, theme::TEXT_DIM);
        draw_rectangle(x + 36.0, row_y, width, 12.0, theme::PANEL);
        draw_rectangle(x + 36.0, row_y, width * value as f32, 12.0, color);
    }
    // Steering: centred bar, left of centre = left.
    let row_y = y + 4.0 * 20.0;
    let mid = x + 36.0 + width / 2.0;
    text("str", x, row_y + 11.0, 18.0, theme::TEXT_DIM);
    draw_rectangle(x + 36.0, row_y, width, 12.0, theme::PANEL);
    let w = (c.steer as f32) * width / 2.0;
    draw_rectangle(mid - w.max(0.0), row_y, w.abs(), 12.0, theme::NEON_PINK);
    draw_line(mid, row_y - 2.0, mid, row_y + 14.0, 1.0, theme::TEXT_DIM);
}
