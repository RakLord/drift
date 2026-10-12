//! Physics telemetry: force/velocity vectors in the world, and a panel of
//! per-wheel friction circles plus drivetrain readouts.

use common::Vec2;
use macroquad::prelude::*;
use sim::car::tyre;
use sim::car::{CarParams, CarState};

use crate::render::{draw_arrow, text, theme};

/// Metres of arrow per newton of tyre force.
const FORCE_SCALE: f64 = 1.0 / 2500.0;
/// Metres of arrow per m/s of velocity.
const VELOCITY_SCALE: f64 = 0.25;

/// World-space vectors. Call with the world camera active.
pub fn draw_world(state: &CarState, params: &CarParams) {
    let at = |local: Vec2| state.position + local.rotate(state.heading);
    for (wheel, offset) in state.wheels.iter().zip(params.body.wheel_offsets()) {
        let from = at(offset);
        let force = wheel.force.rotate(state.heading) * FORCE_SCALE;
        draw_arrow(from, from + force, 0.07, theme::slip_color(wheel.combined_slip));
    }
    draw_arrow(state.position, state.position + state.velocity * VELOCITY_SCALE, 0.08, theme::NEON_CYAN);
}

/// Screen-space panel. Friction circles are laid out like the car seen from
/// above, nose up: each ring is that tyre's grip limit, the dot its force.
pub fn draw_panel(state: &CarState, params: &CarParams) {
    let (x, y, w, h) = (screen_width() - 300.0, 16.0, 284.0, 380.0);
    draw_rectangle(x, y, w, h, theme::PANEL);
    text("TELEMETRY", x + 12.0, y + 24.0, 22.0, theme::NEON_CYAN);

    let radius = 42.0;
    let centers = [
        vec2(x + 72.0, y + 90.0),
        vec2(x + 212.0, y + 90.0),
        vec2(x + 72.0, y + 230.0),
        vec2(x + 212.0, y + 230.0),
    ];
    for (wheel, c) in state.wheels.iter().zip(centers) {
        let limit = tyre::friction_coefficient(&params.tyre, wheel.load) * wheel.load;
        let color = theme::slip_color(wheel.combined_slip);
        draw_circle_lines(c.x, c.y, radius, 1.5, theme::TEXT_DIM);
        if limit > 1.0 {
            // Car frame x forward -> screen up; y left -> screen left.
            let f = wheel.force / limit;
            let dot = vec2(c.x - (f.y * f64::from(radius)) as f32, c.y - (f.x * f64::from(radius)) as f32);
            draw_line(c.x, c.y, dot.x, dot.y, 2.0, color);
            draw_circle(dot.x, dot.y, 5.0, color);
        }
        let lines = [
            format!("{:.1} kN", wheel.load / 1000.0),
            format!("sr {:+.2}  sa {:+.0}", wheel.slip_ratio, wheel.slip_angle.to_degrees()),
        ];
        for (i, line) in lines.iter().enumerate() {
            text(line, c.x - 62.0, c.y + radius + 18.0 + i as f32 * 16.0, 16.0, theme::TEXT_DIM);
        }
    }

    let d = &state.drivetrain;
    let clutch = if d.clutch_locked { "locked" } else { "slipping" };
    let limiter = if d.limiter_active { "  LIMITER" } else { "" };
    let readouts = [
        format!("rpm {:.0}  clutch {clutch}{limiter}", state.engine_rpm()),
        format!(
            "yaw {:+.2} rad/s  accel {:+.2} {:+.2} g",
            state.yaw_rate,
            state.accel.x / sim::GRAVITY,
            state.accel.y / sim::GRAVITY
        ),
    ];
    for (i, line) in readouts.iter().enumerate() {
        text(line, x + 12.0, y + h - 30.0 + i as f32 * 18.0, 16.0, theme::TEXT);
    }
}
