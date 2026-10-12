//! Engine torque, idle governor, rev limiter, and clutch capacity.

use common::math::{inverse_lerp, lerp, smoothstep};

use super::params::{ClutchParams, EngineParams};

const RPM_PER_RAD_S: f64 = 60.0 / std::f64::consts::TAU;

pub fn rpm(omega: f64) -> f64 {
    omega * RPM_PER_RAD_S
}

pub fn omega_from_rpm(rpm: f64) -> f64 {
    rpm / RPM_PER_RAD_S
}

/// The throttle the engine actually gets after the idle governor and the rev
/// limiter. `limiter_active` carries the limiter's hysteresis between calls.
pub fn effective_throttle(p: &EngineParams, omega: f64, throttle: f64, limiter_active: &mut bool) -> f64 {
    let rpm = rpm(omega);
    if rpm >= p.rev_limit_rpm {
        *limiter_active = true;
    } else if rpm < p.rev_limit_rpm - p.limiter_hysteresis_rpm {
        *limiter_active = false;
    }
    if *limiter_active {
        return 0.0;
    }
    let idle = ((p.idle_rpm - rpm) / p.idle_governor_band_rpm).clamp(0.0, 1.0);
    throttle.max(idle)
}

/// Net torque at the crank: combustion torque at this throttle minus internal
/// friction (engine braking).
pub fn torque(p: &EngineParams, omega: f64, throttle: f64) -> f64 {
    let rpm = rpm(omega);
    // Fade friction out near 0 rpm so it never spins the engine backwards.
    let friction = (p.friction_base + p.friction_per_krpm * rpm / 1000.0) * smoothstep(0.0, 100.0, rpm);
    throttle * full_throttle_torque(p, rpm) - friction
}

fn full_throttle_torque(p: &EngineParams, rpm: f64) -> f64 {
    let curve = &p.torque_curve;
    let Some(first) = curve.first() else { return 0.0 };
    if rpm <= first[0] {
        return first[1];
    }
    for pair in curve.windows(2) {
        let ([r0, t0], [r1, t1]) = (pair[0], pair[1]);
        if rpm <= r1 {
            return lerp(t0, t1, inverse_lerp(r0, r1, rpm));
        }
    }
    curve[curve.len() - 1][1]
}

/// Share of its grip a clutch keeps at temperature `temp`: all of it while
/// cool, fading to `fade_grip` once fully cooked.
pub fn clutch_fade(p: &ClutchParams, temp: f64) -> f64 {
    let t = inverse_lerp(p.fade_start_temp, p.fade_full_temp, temp).clamp(0.0, 1.0);
    1.0 - (1.0 - p.fade_grip) * t
}

/// Most torque the clutch can pass right now: less with the pedal pressed,
/// and less when it's hot.
pub fn clutch_capacity(p: &ClutchParams, pedal: f64, temp: f64) -> f64 {
    p.max_torque * (1.0 - pedal) * clutch_fade(p, temp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::CarParams;

    #[test]
    fn curve_interpolates_and_clamps() {
        let p = CarParams::default().engine;
        let at = |rpm: f64| full_throttle_torque(&p, rpm);
        assert_eq!(at(500.0), p.torque_curve[0][1]);
        assert_eq!(at(1e6), p.torque_curve.last().unwrap()[1]);
        let (a, b) = (p.torque_curve[0], p.torque_curve[1]);
        assert!((at((a[0] + b[0]) / 2.0) - (a[1] + b[1]) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn limiter_cuts_and_recovers_with_hysteresis() {
        let p = CarParams::default().engine;
        let mut active = false;
        let limit = omega_from_rpm(p.rev_limit_rpm);
        assert_eq!(effective_throttle(&p, limit + 1.0, 1.0, &mut active), 0.0);
        let just_below = omega_from_rpm(p.rev_limit_rpm - p.limiter_hysteresis_rpm / 2.0);
        assert_eq!(effective_throttle(&p, just_below, 1.0, &mut active), 0.0);
        let well_below = omega_from_rpm(p.rev_limit_rpm - p.limiter_hysteresis_rpm * 2.0);
        assert_eq!(effective_throttle(&p, well_below, 1.0, &mut active), 1.0);
    }

    #[test]
    fn idle_governor_props_up_low_revs() {
        let p = CarParams::default().engine;
        let mut active = false;
        assert!(effective_throttle(&p, omega_from_rpm(p.idle_rpm - 200.0), 0.0, &mut active) > 0.0);
        assert_eq!(effective_throttle(&p, omega_from_rpm(p.idle_rpm + 500.0), 0.0, &mut active), 0.0);
    }

    #[test]
    fn clutch_capacity_follows_pedal_and_fades_when_hot() {
        let p = CarParams::default().clutch;
        let cool = p.ambient_temp;
        assert_eq!(clutch_capacity(&p, 0.0, cool), p.max_torque);
        assert_eq!(clutch_capacity(&p, 1.0, cool), 0.0);
        assert!((clutch_capacity(&p, 0.0, p.fade_full_temp + 50.0) - p.max_torque * p.fade_grip).abs() < 1e-9);
        let half = (p.fade_start_temp + p.fade_full_temp) / 2.0;
        assert!(clutch_capacity(&p, 0.0, half) < p.max_torque);
    }
}
