//! Weight transfer.
//!
//! Acceleration tries to shift load between the wheels (the ideal steady-state
//! amount is `mass * accel * cg_height`). Instead of applying that instantly,
//! the roll and pitch moments chase it through a damped spring, which is
//! roughly what real suspension does. That delay is what makes weight-transfer
//! techniques (feints, lift-off rotation) work.

use std::f64::consts::TAU;

use common::Vec2;

use super::params::{BodyParams, SuspensionParams};
use super::wheel::{FL, FR, RL, RR};

#[derive(Clone, Copy, Debug, Default)]
pub struct SuspensionState {
    /// Lateral load-transfer moment, Nm. Positive loads the right-hand wheels
    /// (what happens when accelerating to the left).
    pub roll_moment: f64,
    pub roll_rate: f64,
    /// Longitudinal load-transfer moment, Nm. Positive loads the rear wheels.
    pub pitch_moment: f64,
    pub pitch_rate: f64,
}

impl SuspensionState {
    /// Advance towards the load transfer implied by `accel` (car frame, m/s^2).
    pub fn update(&mut self, susp: &SuspensionParams, body: &BodyParams, accel: Vec2, dt: f64) {
        let lever = body.mass * body.cg_height;
        spring_towards(
            &mut self.roll_moment,
            &mut self.roll_rate,
            lever * accel.y,
            susp.roll_frequency_hz,
            susp.roll_damping_ratio,
            dt,
        );
        spring_towards(
            &mut self.pitch_moment,
            &mut self.pitch_rate,
            lever * accel.x,
            susp.pitch_frequency_hz,
            susp.pitch_damping_ratio,
            dt,
        );
    }

    /// Per-wheel normal loads, indexed FL, FR, RL, RR. Never negative: a wheel
    /// that would go negative has lifted.
    pub fn wheel_loads(&self, susp: &SuspensionParams, body: &BodyParams) -> [f64; 4] {
        let mut loads = body.static_loads();

        let pitch_each = self.pitch_moment / body.wheelbase / 2.0;
        loads[FL] -= pitch_each;
        loads[FR] -= pitch_each;
        loads[RL] += pitch_each;
        loads[RR] += pitch_each;

        let front_roll = self.roll_moment * susp.front_roll_share / body.track_width_front;
        let rear_roll = self.roll_moment * (1.0 - susp.front_roll_share) / body.track_width_rear;
        loads[FL] -= front_roll;
        loads[FR] += front_roll;
        loads[RL] -= rear_roll;
        loads[RR] += rear_roll;

        loads.map(|load| load.max(0.0))
    }
}

/// One semi-implicit step of a damped spring pulling `value` towards `target`.
fn spring_towards(value: &mut f64, rate: &mut f64, target: f64, frequency_hz: f64, damping: f64, dt: f64) {
    let omega = TAU * frequency_hz;
    let accel = omega * omega * (target - *value) - 2.0 * damping * omega * *rate;
    *rate += accel * dt;
    *value += *rate * dt;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GRAVITY;
    use crate::car::CarParams;

    #[test]
    fn braking_loads_the_front_and_settles() {
        let params = CarParams::default();
        let mut state = SuspensionState::default();
        let braking = Vec2::new(-GRAVITY, 0.0);
        for _ in 0..2000 {
            state.update(&params.suspension, &params.body, braking, 1.0 / 480.0);
        }
        let loads = state.wheel_loads(&params.suspension, &params.body);
        let static_loads = params.body.static_loads();
        assert!(loads[FL] > static_loads[FL] && loads[RL] < static_loads[RL]);

        let expected = params.body.mass * GRAVITY * params.body.cg_height / params.body.wheelbase / 2.0;
        assert!((loads[FL] - static_loads[FL] - expected).abs() < 1.0);
    }

    #[test]
    fn turning_left_loads_the_right_side() {
        let params = CarParams::default();
        let state = SuspensionState { roll_moment: 3000.0, ..Default::default() };
        let loads = state.wheel_loads(&params.suspension, &params.body);
        assert!(loads[FR] > loads[FL] && loads[RR] > loads[RL]);
        let total: f64 = loads.iter().sum();
        assert!((total - params.body.mass * GRAVITY).abs() < 1e-6);
    }
}
