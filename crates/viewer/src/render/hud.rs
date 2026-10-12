//! Driver HUD (screen space): speed, tach, drift angle, input bars.

use macroquad::prelude::*;
use sim::{Controls, World};
use sim::car::{CarParams, CarState};

use super::{text, theme};

/// `view_w`: width of the track area on screen (the HUD stays inside it).
pub fn draw(state: &CarState, params: &CarParams, view_w: f32) {
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
    draw_clutch_temp(state, params, x, h - 50.0, 300.0);
    draw_inputs(&state.controls, view_w - 240.0, h - 160.0);

    if !state.engine_running() {
        let message = "ENGINE STALLED - clutch in, then Y / Select to restart";
        let width = measure_text(message, None, 30, 1.0).width;
        text(message, (view_w - width) / 2.0, h * 0.4, 30.0, theme::SATURATED);
    }
}

/// Clutch temperature: grey when cool, orange as it nears fade, red once
/// it's fading.
fn draw_clutch_temp(state: &CarState, params: &CarParams, x: f32, y: f32, width: f32) {
    let c = &params.clutch;
    let temp = state.drivetrain.clutch_temp;
    let fraction = ((temp - c.ambient_temp) / (c.fade_full_temp - c.ambient_temp)).clamp(0.0, 1.0) as f32;
    let color = if temp >= c.fade_start_temp {
        theme::SATURATED
    } else if temp >= c.fade_start_temp * 0.75 {
        theme::SODIUM
    } else {
        theme::TEXT_DIM
    };
    draw_rectangle(x, y, width, 6.0, theme::PANEL);
    draw_rectangle(x, y, width * fraction, 6.0, color);
    text(&format!("clutch {temp:.0}°C"), x + width + 10.0, y + 7.0, 18.0, color);
}

/// Track name and position along the lap, top centre.
pub fn draw_track_info(world: &World, view_w: f32) {
    let track = &world.track;
    let lap = (world.progress / track.length).floor().max(0.0) as u32 + 1;
    let line = format!(
        "{}   lap {lap}   {:.0} / {:.0} m",
        track.def.name, world.place.distance, track.length
    );
    text(&line, view_w / 2.0 - 140.0, 20.0, 18.0, theme::TEXT_DIM);
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
