use common::Vec2;
use macroquad::prelude::Color;
use sim::car::wheel;
use sim::car::{CarParams, CarState};

use super::effects::damage::{BUMPER_DEPTH, Bumper, Damage, bumper_size};
use super::{draw_box, light, theme};

/// Headlight beams: reach and spread (metres), strength, and how far each
/// beam angles out from straight ahead.
const BEAM_LENGTH: f64 = 22.0;
const BEAM_WIDTH: f64 = 11.0;
const BEAM_ALPHA: f32 = 0.16;
const BEAM_SPLAY: f64 = 0.05;
/// Tail lights' glow, lights on / braking.
const TAIL_GLOW: (f64, f32) = (1.0, 0.35);
const BRAKE_GLOW: (f64, f32) = (2.6, 0.75);

/// Draw the car at `state`'s position and heading (already interpolated by
/// the caller), with whatever bumpers it still has.
pub fn draw(state: &CarState, params: &CarParams, damage: &Damage) {
    draw_painted(state, params, damage, theme::CAR_BODY, theme::CAR_ROOF, 1.0);
}

/// Draw with its own body and roof colours, and see-through by `alpha`
/// (the live multi-car view colours cars by island and fades finished ones).
pub fn draw_painted(state: &CarState, params: &CarParams, damage: &Damage, paint: Color, roof: Color, alpha: f32) {
    let c = |color: Color| Color { a: color.a * alpha, ..color };
    let body = &params.body;
    let at = |local: Vec2| state.position + local.rotate(state.heading);
    let half = body.length / 2.0;
    let shell = Vec2::new(body.length - 2.0 * BUMPER_DEPTH, body.width * 0.92);

    draw_box(at(Vec2::new(-0.15, -0.2)), Vec2::new(body.length, body.width), state.heading, c(theme::CAR_SHADOW));

    let tyre_size = Vec2::new(params.wheel.radius * 2.0, 0.24);
    for (i, position) in state.wheel_positions(params).into_iter().enumerate() {
        let steer = if wheel::is_front(i) { state.steer_angle } else { 0.0 };
        draw_box(position, tyre_size, state.heading + steer, c(theme::TYRE));
    }

    draw_box(state.position, shell, state.heading, c(paint));
    for (bumper, end) in [(damage.front, 1.0), (damage.rear, -1.0)] {
        let centre = Vec2::new(end * (half - BUMPER_DEPTH / 2.0), 0.0);
        match bumper {
            Bumper::Fine => draw_box(at(centre), bumper_size(params), state.heading, c(paint)),
            // Hanging off at one corner.
            Bumper::Loose => draw_box(
                at(centre + Vec2::new(end * 0.06, 0.12)),
                bumper_size(params),
                state.heading + end * 0.12,
                c(paint),
            ),
            // Bare crash bar where the bumper was.
            Bumper::Gone => draw_box(
                at(Vec2::new(end * (half - BUMPER_DEPTH * 0.8), 0.0)),
                Vec2::new(0.08, body.width * 0.7),
                state.heading,
                c(theme::CRASH_BAR),
            ),
        }
    }

    // Cabin from above: windscreen, roof, rear window.
    draw_box(at(Vec2::new(0.3, 0.0)), Vec2::new(0.6, body.width * 0.74), state.heading, c(theme::CAR_GLASS));
    draw_box(at(Vec2::new(-0.45, 0.0)), Vec2::new(0.9, body.width * 0.7), state.heading, c(roof));
    draw_box(at(Vec2::new(-1.1, 0.0)), Vec2::new(0.4, body.width * 0.66), state.heading, c(theme::CAR_GLASS));

    if damage.rear != Bumper::Gone {
        let braking = braking(state);
        let light = c(if braking { theme::BRAKE_LIGHT_ON } else { theme::BRAKE_LIGHT_OFF });
        for side in [-1.0, 1.0] {
            let at_light = at(Vec2::new(-half + BUMPER_DEPTH + 0.06, side * body.width * 0.33));
            draw_box(at_light, Vec2::new(0.1, 0.35), state.heading, light);
        }
    }
}

fn braking(state: &CarState) -> bool {
    state.controls.brake > 0.05 || state.controls.handbrake > 0.05
}

/// Headlight beams on the road. Call inside the ground `light::pass`,
/// before the cars, so a pack's beams light the road, not each other's
/// paint. `alpha` fades them with the car.
pub fn draw_beams(state: &CarState, params: &CarParams, alpha: f32) {
    let body = &params.body;
    let at = |local: Vec2| state.position + local.rotate(state.heading);
    for side in [-1.0, 1.0] {
        let lamp = at(Vec2::new(body.length / 2.0 - BUMPER_DEPTH, side * body.width * 0.34));
        light::cone(lamp, state.heading + side * BEAM_SPLAY, BEAM_LENGTH, BEAM_WIDTH, Color { a: BEAM_ALPHA * alpha, ..theme::HEADLIGHT });
    }
}

/// The tail lights' glow, flaring on the brakes. Call inside the `light::pass`
/// after the cars.
pub fn draw_tail_lights(state: &CarState, params: &CarParams, damage: &Damage, alpha: f32) {
    if damage.rear == Bumper::Gone {
        return;
    }
    let body = &params.body;
    let half = body.length / 2.0;
    let at = |local: Vec2| state.position + local.rotate(state.heading);
    let (radius, strength) = if braking(state) { BRAKE_GLOW } else { TAIL_GLOW };
    for side in [-1.0, 1.0] {
        let lamp = at(Vec2::new(-half + BUMPER_DEPTH, side * body.width * 0.33));
        light::glow(lamp, radius, Color { a: strength * alpha, ..theme::TAIL_LIGHT });
    }
}
