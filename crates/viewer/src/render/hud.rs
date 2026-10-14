//! Driver HUD (screen space): a dash centred along the bottom of the track
//! view with speed, drift angle, tach, clutch temperature, a steering wheel
//! and the pedals.

use macroquad::prelude::*;
use sim::{Controls, World};
use sim::car::{CarParams, CarState};

use super::{text, theme};

pub const DASH_W: f32 = 640.0;
pub const DASH_H: f32 = 140.0;

/// How far the drawn wheel turns at full steer, each way. Past 360 the top
/// stripe stops telling you which way round it is.
const WHEEL_DEG: f32 = 270.0;
const MPH_PER_MS: f64 = 2.236_936;

/// The dash, centred in a track view `view_w` wide, its bottom edge at `bottom`.
pub fn draw_at(state: &CarState, params: &CarParams, view_w: f32, bottom: f32) {
    let x = ((view_w - DASH_W) / 2.0).max(16.0);
    let y = bottom - DASH_H;
    draw_rectangle(x, y, DASH_W, DASH_H, theme::PANEL);

    let mph = state.speed() * MPH_PER_MS;
    text(&format!("{mph:.0}"), x + 20.0, y + 56.0, 64.0, theme::TEXT);
    text("mph", x + 24.0, y + 76.0, 18.0, theme::TEXT_DIM);

    let drift = state.body_slip_angle().to_degrees().abs();
    let drift_color = if drift > 10.0 { theme::NEON_PINK } else { theme::TEXT_DIM };
    text(&format!("{drift:.0}°"), x + 150.0, y + 56.0, 48.0, drift_color);
    text("angle", x + 152.0, y + 76.0, 18.0, theme::TEXT_DIM);

    draw_tach(state, params, x + 20.0, y + 92.0, 170.0);
    draw_clutch_temp(state, params, x + 20.0, y + 114.0, 170.0);
    draw_wheel(state, params, vec2(x + 372.0, y + 60.0), 42.0);
    draw_pedals(&state.controls, x + 456.0, y + 16.0);

    if !state.engine_running() {
        let message = "ENGINE STALLED - clutch in, then Y / Select to restart";
        let width = measure_text(message, None, 30, 1.0).width;
        text(message, (view_w - width) / 2.0, bottom * 0.4, 30.0, theme::SATURATED);
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
    draw_rectangle(x, y, width, 6.0, theme::PANEL_SOLID);
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
    draw_rectangle(x, y, width, 10.0, theme::PANEL_SOLID);
    let color = if fraction > 0.92 { theme::SATURATED } else { theme::SODIUM };
    draw_rectangle(x, y, width * fraction, 10.0, color);
    let label = format!("{:.0} rpm", state.engine_rpm());
    text(&label, x + width + 10.0, y + 10.0, 20.0, theme::TEXT_DIM);
}

/// Steering wheel turned by the steer request, with a dot outside the rim
/// where the road wheels actually are (the rack is rate limited, so it trails a
/// quick flick). Road-wheel angle underneath.
fn draw_wheel(state: &CarState, params: &CarParams, centre: Vec2, radius: f32) {
    // Clockwise from 12 o'clock, so left (positive) steer turns it anticlockwise.
    let at = |deg: f32, r: f32| {
        let a = deg.to_radians();
        centre + vec2(a.sin(), -a.cos()) * r
    };
    let turn = -(state.controls.steer as f32) * WHEEL_DEG;

    for spoke in [90.0, -90.0, 180.0] {
        let end = at(turn + spoke, radius);
        draw_line(centre.x, centre.y, end.x, end.y, 7.0, theme::CONCRETE_DARK);
    }
    draw_circle(centre.x, centre.y, 12.0, theme::CONCRETE_DARK);
    draw_circle_lines(centre.x, centre.y, radius, 8.0, theme::CONCRETE);
    let (inner, outer) = (at(turn, radius - 6.0), at(turn, radius + 6.0));
    draw_line(inner.x, inner.y, outer.x, outer.y, 8.0, theme::SODIUM);

    let lock = params.steering.max_lock();
    let rack = if lock > 0.0 { (state.steer_angle / lock) as f32 } else { 0.0 };
    let dot = at(-rack * WHEEL_DEG, radius + 11.0);
    draw_circle(dot.x, dot.y, 3.5, theme::NEON_CYAN);

    let deg = state.steer_angle.to_degrees();
    let label = if deg.abs() < 0.5 {
        "0°".to_owned()
    } else {
        format!("{} {:.0}°", if deg > 0.0 { "L" } else { "R" }, deg.abs())
    };
    let width = measure_text(&label, None, 18, 1.0).width;
    text(&label, centre.x - width / 2.0, centre.y + radius + 24.0, 18.0, theme::TEXT_DIM);
}

/// Clutch, brake and throttle (Assetto Corsa order), then the handbrake set
/// apart. Each fills from the bottom, with its percentage underneath.
fn draw_pedals(c: &Controls, x: f32, y: f32) {
    const BAR_W: f32 = 24.0;
    const BAR_H: f32 = 86.0;
    let pedals = [
        ("C", c.clutch, theme::NEON_CYAN, 0.0),
        ("B", c.brake, theme::SATURATED, 40.0),
        ("T", c.throttle, theme::GRIP, 80.0),
        ("HB", c.handbrake, theme::SODIUM, 140.0),
    ];
    for (label, value, color, offset) in pedals {
        let bx = x + offset;
        let fill = BAR_H * value as f32;
        draw_rectangle(bx, y, BAR_W, BAR_H, theme::PANEL_SOLID);
        draw_rectangle(bx, y + BAR_H - fill, BAR_W, fill, color);
        draw_rectangle_lines(bx, y, BAR_W, BAR_H, 1.0, theme::TEXT_DIM);
        let centred = |s: &str, ty: f32, size: u16, color: Color| {
            let width = measure_text(s, None, size, 1.0).width;
            text(s, bx + (BAR_W - width) / 2.0, ty, f32::from(size), color);
        };
        centred(label, y + BAR_H + 18.0, 18, theme::TEXT_DIM);
        centred(&format!("{:.0}", value * 100.0), y + BAR_H + 34.0, 16, theme::TEXT_DIM);
    }
}
