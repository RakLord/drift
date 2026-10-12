//! Dev tool: a scripted driver that kicks into a left-hand drift and holds it,
//! the same way the physics tests do. Handy for checking visuals without a pad.

use sim::Controls;
use sim::car::{CarParams, CarState};

const TARGET_ANGLE_DEG: f64 = -25.0;

pub struct Autopilot {
    time: f64,
}

impl Autopilot {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    pub fn controls(&mut self, state: &CarState, params: &CarParams, dt: f64) -> Controls {
        self.time += dt;
        let t = self.time;
        if t < 1.5 {
            return Controls { throttle: 1.0, ..Default::default() };
        }
        if t < 2.0 {
            return Controls { steer: 0.4, throttle: 1.0, clutch: if t < 1.8 { 1.0 } else { 0.0 }, ..Default::default() };
        }
        let beta = state.body_slip_angle();
        let target = TARGET_ANGLE_DEG.to_radians();
        let steer = beta + (beta - target) - 0.1 * state.yaw_rate;
        Controls {
            steer: steer / params.steering.max_lock(),
            throttle: (0.6 + (beta - target)).clamp(0.0, 1.0),
            ..Default::default()
        }
    }
}
