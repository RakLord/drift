use common::math::approach;

use super::params::SteeringParams;

/// Move the road wheels towards the requested steer, limited by rack speed.
/// `input` is -1..1 (positive = left).
pub fn update(p: &SteeringParams, current: f64, input: f64, dt: f64) -> f64 {
    approach(current, input * p.max_lock(), p.rate() * dt)
}
