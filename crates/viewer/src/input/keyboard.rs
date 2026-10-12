use common::math::approach;
use macroquad::prelude::*;
use sim::Controls;

/// Keyboard driving. Steering ramps instead of snapping, since keys are on/off.
#[derive(Default)]
pub struct Keyboard {
    steer: f64,
    active: bool,
}

const STEER_IN_RATE: f64 = 2.5;
const STEER_OUT_RATE: f64 = 5.0;

const LEFT: [KeyCode; 2] = [KeyCode::Left, KeyCode::A];
const RIGHT: [KeyCode; 2] = [KeyCode::Right, KeyCode::D];
const THROTTLE: [KeyCode; 2] = [KeyCode::Up, KeyCode::W];
const BRAKE: [KeyCode; 2] = [KeyCode::Down, KeyCode::S];
const HANDBRAKE: [KeyCode; 1] = [KeyCode::Space];
const CLUTCH: [KeyCode; 2] = [KeyCode::LeftShift, KeyCode::RightShift];

fn any_down(keys: &[KeyCode]) -> bool {
    keys.iter().any(|&k| is_key_down(k))
}

fn pedal(keys: &[KeyCode]) -> f64 {
    if any_down(keys) { 1.0 } else { 0.0 }
}

impl Keyboard {
    pub fn update(&mut self, dt: f64) -> Controls {
        let target = pedal(&LEFT) - pedal(&RIGHT);
        let rate = if target == 0.0 || target.signum() != self.steer.signum() { STEER_OUT_RATE } else { STEER_IN_RATE };
        self.steer = approach(self.steer, target, rate * dt);

        let all = [&LEFT[..], &RIGHT, &THROTTLE, &BRAKE, &HANDBRAKE, &CLUTCH];
        self.active = all.iter().any(|keys| any_down(keys)) || self.steer != 0.0;

        Controls {
            steer: self.steer,
            throttle: pedal(&THROTTLE),
            brake: pedal(&BRAKE),
            handbrake: pedal(&HANDBRAKE),
            clutch: pedal(&CLUTCH),
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }
}
