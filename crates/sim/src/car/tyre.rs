//! Tyre model: combined-slip friction with a peak and a fall-off to sliding grip.
//!
//! Slip is measured as the contact patch's sliding velocity divided by a
//! reference speed. Longitudinal and lateral slip are each scaled by the slip
//! where they peak, so one normalised value `s` describes the whole patch:
//! below 1 the tyre is gripping, above 1 it is sliding. Grip follows a curve
//! of `s`, and the force points against the slip. This holds up at any angle,
//! including full 90 degree slides and spins, which drifting needs.

use common::Vec2;
use common::math::smoothstep;

use super::params::TyreParams;

/// Result of evaluating one tyre. Vectors are in the wheel frame
/// (x along the wheel's rolling direction, y to its left).
#[derive(Clone, Copy, Debug, Default)]
pub struct TyreOutput {
    pub force: Vec2,
    /// (surface speed - ground speed) / ground speed. Positive = wheelspin,
    /// -1 = locked.
    pub slip_ratio: f64,
    /// Angle between where the wheel points and where it travels. Positive
    /// when the patch slides to the left.
    pub slip_angle: f64,
    /// Normalised combined slip: < 1 gripping, > 1 past the peak (sliding).
    pub combined_slip: f64,
    /// How fast the contact patch slides over the ground, m/s.
    pub slip_speed: f64,
}

/// Evaluate a tyre.
///
/// - `velocity`: contact patch velocity over the ground, wheel frame.
/// - `surface_speed`: wheel spin times radius (positive = rolling forward).
/// - `load`: normal load in N.
pub fn evaluate(p: &TyreParams, velocity: Vec2, surface_speed: f64, load: f64) -> TyreOutput {
    let slip_velocity = Vec2::new(surface_speed - velocity.x, -velocity.y);
    let reference = velocity.length().max(surface_speed.abs()).max(p.min_speed);
    let slip = slip_velocity / reference;

    let normalised = Vec2::new(slip.x / p.peak_slip_long, slip.y / p.peak_slip_lat);
    let s = normalised.length();

    let mut out = TyreOutput {
        force: Vec2::ZERO,
        slip_ratio: (surface_speed - velocity.x) / velocity.x.abs().max(p.min_speed),
        slip_angle: velocity.y.atan2(velocity.x.abs()),
        combined_slip: s,
        slip_speed: slip_velocity.length(),
    };
    if load <= 0.0 || s < 1e-9 {
        return out;
    }

    // While gripping, the force follows the stiffness-weighted slip; once
    // sliding, it opposes the actual sliding direction.
    let elastic_dir = normalised / s;
    let sliding_dir = slip.normalize_or_zero();
    let direction = elastic_dir.lerp(sliding_dir, smoothstep(1.0, 2.5, s)).normalize_or_zero();

    out.force = direction * (friction_coefficient(p, load) * load * grip_curve(p, s));
    out
}

/// Peak friction coefficient, reduced slightly as load rises. This is why
/// shifting weight onto one end of the car costs that end grip overall.
pub fn friction_coefficient(p: &TyreParams, load: f64) -> f64 {
    let excess = load / p.reference_load - 1.0;
    (p.mu_peak * (1.0 - p.load_sensitivity * excess)).max(0.2 * p.mu_peak)
}

/// Fraction of peak grip available at normalised slip `s`: rises to 1 at
/// `s = 1`, then falls smoothly towards `slide_ratio`.
pub fn grip_curve(p: &TyreParams, s: f64) -> f64 {
    if s <= 1.0 {
        s * (2.0 - s)
    } else {
        let t = (s - 1.0) / p.falloff;
        p.slide_ratio + (1.0 - p.slide_ratio) / (1.0 + t * t)
    }
}

/// d(longitudinal force) / d(surface speed), never negative. Used to integrate
/// wheel spin implicitly, which keeps the stiff wheel/tyre coupling stable.
pub fn longitudinal_stiffness(p: &TyreParams, velocity: Vec2, surface_speed: f64, load: f64, base_fx: f64) -> f64 {
    const DELTA: f64 = 0.01;
    let nudged = evaluate(p, velocity, surface_speed + DELTA, load).force.x;
    ((nudged - base_fx) / DELTA).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::CarParams;

    fn tyre() -> TyreParams {
        CarParams::default().tyre
    }

    #[test]
    fn free_rolling_straight_has_no_force() {
        let p = tyre();
        let out = evaluate(&p, Vec2::new(20.0, 0.0), 20.0, 3000.0);
        assert!(out.force.length() < 1e-9);
    }

    #[test]
    fn curve_peaks_at_one_and_settles_at_slide_ratio() {
        let p = tyre();
        assert!((grip_curve(&p, 1.0) - 1.0).abs() < 1e-12);
        assert!(grip_curve(&p, 0.5) < 1.0);
        assert!(grip_curve(&p, 2.0) < 1.0);
        assert!((grip_curve(&p, 1e6) - p.slide_ratio).abs() < 1e-3);
    }

    #[test]
    fn force_opposes_sideways_sliding() {
        let p = tyre();
        let out = evaluate(&p, Vec2::new(0.0, 10.0), 0.0, 3000.0);
        assert!(out.force.y < 0.0);
        assert!(out.force.x.abs() < 1e-6);
        let sliding = friction_coefficient(&p, 3000.0) * 3000.0 * p.slide_ratio;
        assert!((out.force.length() - sliding).abs() / sliding < 0.05);
    }

    #[test]
    fn wheelspin_pushes_forward_and_lockup_pulls_back() {
        let p = tyre();
        let spin = evaluate(&p, Vec2::new(10.0, 0.0), 12.0, 3000.0);
        assert!(spin.force.x > 0.0 && spin.slip_ratio > 0.0);
        let locked = evaluate(&p, Vec2::new(10.0, 0.0), 0.0, 3000.0);
        assert!(locked.force.x < 0.0);
        assert!((locked.slip_ratio + 1.0).abs() < 1e-9);
    }

    #[test]
    fn force_never_exceeds_peak_friction() {
        let p = tyre();
        for vx in [-10.0, 0.0, 0.5, 5.0, 30.0] {
            for vy in [-10.0, -1.0, 0.0, 2.0, 15.0] {
                for surface in [-5.0, 0.0, 5.0, 40.0] {
                    let out = evaluate(&p, Vec2::new(vx, vy), surface, 3000.0);
                    let max = friction_coefficient(&p, 3000.0) * 3000.0;
                    assert!(out.force.length() <= max * (1.0 + 1e-9));
                }
            }
        }
    }

    #[test]
    fn more_load_means_lower_coefficient() {
        let p = tyre();
        assert!(friction_coefficient(&p, 5000.0) < friction_coefficient(&p, 3000.0));
    }
}
