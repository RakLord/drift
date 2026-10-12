use common::Vec2;

use super::drivetrain::DrivetrainState;
use super::engine;
use super::params::CarParams;
use super::suspension::SuspensionState;
use crate::Controls;

/// Per-wheel state. Rear wheels always share one spin rate (welded diff).
#[derive(Clone, Copy, Debug, Default)]
pub struct WheelState {
    /// Spin rate, rad/s. Positive = rolling forward.
    pub omega: f64,
    /// Normal load, N.
    pub load: f64,
    /// Tyre force on the car, car frame, N.
    pub force: Vec2,
    pub slip_ratio: f64,
    pub slip_angle: f64,
    /// < 1 gripping, > 1 sliding past the peak.
    pub combined_slip: f64,
    /// Contact patch sliding speed, m/s. Drives smoke and skid marks.
    pub slip_speed: f64,
}

#[derive(Clone, Debug, Default)]
pub struct CarState {
    pub position: Vec2,
    /// Radians, counter-clockwise from +x.
    pub heading: f64,
    /// CG velocity, world frame, m/s.
    pub velocity: Vec2,
    /// Rad/s, positive = rotating left (counter-clockwise).
    pub yaw_rate: f64,
    /// Actual road-wheel steer angle after rate limiting, radians.
    pub steer_angle: f64,
    /// Indexed FL, FR, RL, RR.
    pub wheels: [WheelState; 4],
    pub drivetrain: DrivetrainState,
    pub suspension: SuspensionState,
    /// What an accelerometer at the CG reads, car frame, m/s^2.
    pub accel: Vec2,
    /// Controls as the car received them (after clamping).
    pub controls: Controls,
}

impl CarState {
    /// A car standing still with the engine idling.
    pub fn at_rest(params: &CarParams, position: Vec2, heading: f64) -> Self {
        let mut state = Self { position, heading, ..Default::default() };
        state.drivetrain.engine_omega = engine::omega_from_rpm(params.engine.idle_rpm);
        for (wheel, load) in state.wheels.iter_mut().zip(params.body.static_loads()) {
            wheel.load = load;
        }
        state
    }

    /// A car rolling straight ahead at `speed` with the clutch locked, as if
    /// it had been cruising.
    pub fn rolling(params: &CarParams, position: Vec2, heading: f64, speed: f64) -> Self {
        let mut state = Self::at_rest(params, position, heading);
        state.velocity = Vec2::from_angle(heading) * speed;
        let omega = speed / params.wheel.radius;
        for wheel in &mut state.wheels {
            wheel.omega = omega;
        }
        let engine = omega * params.drivetrain.ratio;
        if engine >= engine::omega_from_rpm(params.clutch.bite_rpm) {
            state.drivetrain.engine_omega = engine;
            state.drivetrain.clutch_locked = true;
        }
        state
    }

    /// CG velocity in the car frame (x forward, y left).
    pub fn local_velocity(&self) -> Vec2 {
        self.velocity.to_local(self.heading)
    }

    pub fn speed(&self) -> f64 {
        self.velocity.length()
    }

    /// Angle between where the car points and where it travels: the drift
    /// angle. Negative in a left-hand drift (nose pointing left of travel,
    /// rear out to the right), positive in a right-hand one. Zero when nearly
    /// stationary.
    pub fn body_slip_angle(&self) -> f64 {
        let v = self.local_velocity();
        if v.length() < 0.5 { 0.0 } else { v.y.atan2(v.x) }
    }

    /// World positions of the contact patches, indexed FL, FR, RL, RR.
    pub fn wheel_positions(&self, params: &CarParams) -> [Vec2; 4] {
        params.body.wheel_offsets().map(|offset| self.position + offset.rotate(self.heading))
    }

    pub fn engine_rpm(&self) -> f64 {
        engine::rpm(self.drivetrain.engine_omega)
    }
}
