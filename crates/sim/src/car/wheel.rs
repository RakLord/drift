//! Wheel indexing, contact patch kinematics, and wheel spin integration.

use common::Vec2;
use common::math::approach;

pub const FL: usize = 0;
pub const FR: usize = 1;
pub const RL: usize = 2;
pub const RR: usize = 3;

pub fn is_front(index: usize) -> bool {
    index == FL || index == FR
}

/// Velocity of a contact patch over the ground, in that wheel's frame.
///
/// `body_velocity` is the CG velocity in the car frame, `offset` the patch
/// position relative to the CG (car frame), `steer` the wheel's steer angle.
pub fn contact_velocity(body_velocity: Vec2, yaw_rate: f64, offset: Vec2, steer: f64) -> Vec2 {
    let point = body_velocity + offset.perp() * yaw_rate;
    point.rotate(-steer)
}

/// Inputs for advancing one spinning body (a wheel, or the driven axle) by a
/// substep.
pub struct SpinStep {
    pub inertia: f64,
    pub omega: f64,
    /// Torque driving the spin (engine/clutch), Nm.
    pub drive_torque: f64,
    pub radius: f64,
    /// Total longitudinal tyre force on this body at the current spin, N.
    pub tyre_force: f64,
    /// d(tyre force)/d(surface speed), summed over the tyres on this body.
    pub tyre_stiffness: f64,
    /// Brakes and rolling resistance: always resist spin, never reverse it.
    pub friction_torque: f64,
}

impl SpinStep {
    /// New spin rate after `dt`.
    ///
    /// The tyre force is linearised around the current spin and solved
    /// implicitly: the wheel/tyre coupling is far too stiff for plain explicit
    /// integration at low speed. Friction torque is then applied as a pull
    /// towards zero spin, so a locked wheel stays locked rather than jittering.
    pub fn solve(&self, dt: f64) -> f64 {
        // d(tyre torque)/d(omega) = radius^2 * d(force)/d(surface speed).
        let k = self.tyre_stiffness * self.radius * self.radius;
        let free = (self.inertia * self.omega
            + dt * (self.drive_torque - self.radius * self.tyre_force + k * self.omega))
            / (self.inertia + dt * k);
        approach(free, 0.0, self.friction_torque * dt / self.inertia)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_adds_sideways_velocity_at_front_wheels() {
        // Spinning left (CCW) moves the front of the car to the left.
        let v = contact_velocity(Vec2::ZERO, 1.0, Vec2::new(1.5, 0.0), 0.0);
        assert!((v - Vec2::new(0.0, 1.5)).length() < 1e-12);
    }

    #[test]
    fn steered_wheel_sees_velocity_in_its_own_frame() {
        let steer = 0.3;
        let v = contact_velocity(Vec2::from_angle(steer) * 10.0, 0.0, Vec2::ZERO, steer);
        assert!((v - Vec2::new(10.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn friction_never_reverses_spin() {
        let step = SpinStep {
            inertia: 1.0,
            omega: 0.5,
            drive_torque: 0.0,
            radius: 0.3,
            tyre_force: 0.0,
            tyre_stiffness: 0.0,
            friction_torque: 1000.0,
        };
        assert_eq!(step.solve(0.01), 0.0);
    }
}
