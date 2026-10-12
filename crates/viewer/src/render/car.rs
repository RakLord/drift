use common::Vec2;
use sim::car::wheel;
use sim::car::{CarParams, CarState};

use super::{draw_box, theme};

/// Draw the car at `state`'s position and heading (already interpolated by
/// the caller).
pub fn draw(state: &CarState, params: &CarParams) {
    let body = &params.body;
    let size = Vec2::new(body.length, body.width);
    let at = |local: Vec2| state.position + local.rotate(state.heading);

    draw_box(at(Vec2::new(-0.15, -0.2)), size, state.heading, theme::CAR_SHADOW);

    let tyre_size = Vec2::new(params.wheel.radius * 2.0, 0.24);
    for (i, offset) in body.wheel_offsets().into_iter().enumerate() {
        let steer = if wheel::is_front(i) { state.steer_angle } else { 0.0 };
        draw_box(at(offset), tyre_size, state.heading + steer, theme::TYRE);
    }

    draw_box(state.position, Vec2::new(size.x, size.y * 0.92), state.heading, theme::CAR_BODY);
    // Cabin from above: windscreen, roof, rear window.
    draw_box(at(Vec2::new(0.3, 0.0)), Vec2::new(0.6, body.width * 0.74), state.heading, theme::CAR_GLASS);
    draw_box(at(Vec2::new(-0.45, 0.0)), Vec2::new(0.9, body.width * 0.7), state.heading, theme::CAR_ROOF);
    draw_box(at(Vec2::new(-1.1, 0.0)), Vec2::new(0.4, body.width * 0.66), state.heading, theme::CAR_GLASS);

    let braking = state.controls.brake > 0.05 || state.controls.handbrake > 0.05;
    let light = if braking { theme::BRAKE_LIGHT_ON } else { theme::BRAKE_LIGHT_OFF };
    let rear = -body.length / 2.0 + 0.06;
    for side in [-1.0, 1.0] {
        draw_box(at(Vec2::new(rear, side * body.width * 0.33)), Vec2::new(0.1, 0.35), state.heading, light);
    }
}
