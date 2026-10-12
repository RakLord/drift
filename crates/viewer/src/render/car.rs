use common::Vec2;
use sim::car::wheel;
use sim::car::{CarParams, CarState};

use super::effects::damage::{BUMPER_DEPTH, Bumper, Damage, bumper_size};
use super::{draw_box, theme};

/// Draw the car at `state`'s position and heading (already interpolated by
/// the caller), with whatever bumpers it still has.
pub fn draw(state: &CarState, params: &CarParams, damage: &Damage) {
    let body = &params.body;
    let at = |local: Vec2| state.position + local.rotate(state.heading);
    let half = body.length / 2.0;
    let shell = Vec2::new(body.length - 2.0 * BUMPER_DEPTH, body.width * 0.92);

    draw_box(at(Vec2::new(-0.15, -0.2)), Vec2::new(body.length, body.width), state.heading, theme::CAR_SHADOW);

    let tyre_size = Vec2::new(params.wheel.radius * 2.0, 0.24);
    for (i, position) in state.wheel_positions(params).into_iter().enumerate() {
        let steer = if wheel::is_front(i) { state.steer_angle } else { 0.0 };
        draw_box(position, tyre_size, state.heading + steer, theme::TYRE);
    }

    draw_box(state.position, shell, state.heading, theme::CAR_BODY);
    for (bumper, end) in [(damage.front, 1.0), (damage.rear, -1.0)] {
        let centre = Vec2::new(end * (half - BUMPER_DEPTH / 2.0), 0.0);
        match bumper {
            Bumper::Fine => draw_box(at(centre), bumper_size(params), state.heading, theme::CAR_BODY),
            // Hanging off at one corner.
            Bumper::Loose => draw_box(
                at(centre + Vec2::new(end * 0.06, 0.12)),
                bumper_size(params),
                state.heading + end * 0.12,
                theme::CAR_BODY,
            ),
            // Bare crash bar where the bumper was.
            Bumper::Gone => draw_box(
                at(Vec2::new(end * (half - BUMPER_DEPTH * 0.8), 0.0)),
                Vec2::new(0.08, body.width * 0.7),
                state.heading,
                theme::CRASH_BAR,
            ),
        }
    }

    // Cabin from above: windscreen, roof, rear window.
    draw_box(at(Vec2::new(0.3, 0.0)), Vec2::new(0.6, body.width * 0.74), state.heading, theme::CAR_GLASS);
    draw_box(at(Vec2::new(-0.45, 0.0)), Vec2::new(0.9, body.width * 0.7), state.heading, theme::CAR_ROOF);
    draw_box(at(Vec2::new(-1.1, 0.0)), Vec2::new(0.4, body.width * 0.66), state.heading, theme::CAR_GLASS);

    if damage.rear != Bumper::Gone {
        let braking = state.controls.brake > 0.05 || state.controls.handbrake > 0.05;
        let light = if braking { theme::BRAKE_LIGHT_ON } else { theme::BRAKE_LIGHT_OFF };
        for side in [-1.0, 1.0] {
            let at_light = at(Vec2::new(-half + BUMPER_DEPTH + 0.06, side * body.width * 0.33));
            draw_box(at_light, Vec2::new(0.1, 0.35), state.heading, light);
        }
    }
}
