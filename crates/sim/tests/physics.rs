//! Behaviour tests: does the car do what a real car would, in plausible numbers?
//!
//! These drive the default car (config/car.toml) with simple scripted inputs.
//! Ranges are deliberately generous; they catch broken physics, not tuning.

use common::Vec2;
use common::math::rad_to_deg;
use sim::car::wheel::{RL, RR};
use sim::car::{Car, CarParams, CarState};
use sim::{Controls, GRAVITY};

const DT: f64 = 1.0 / 120.0;
const KMH: f64 = 1.0 / 3.6;

fn car_at_rest() -> Car {
    Car::new(CarParams::default(), Vec2::ZERO, 0.0)
}

fn car_rolling(speed: f64) -> Car {
    let params = CarParams::default();
    let state = CarState::rolling(&params, Vec2::ZERO, 0.0, speed);
    Car { params, state }
}

/// Run until `done` returns true or `max_secs` pass. Returns elapsed seconds.
fn drive(car: &mut Car, max_secs: f64, mut controls: impl FnMut(&CarState, f64) -> Controls) -> f64 {
    drive_until(car, max_secs, &mut controls, |_| false)
}

fn drive_until(
    car: &mut Car,
    max_secs: f64,
    controls: &mut impl FnMut(&CarState, f64) -> Controls,
    done: impl Fn(&CarState) -> bool,
) -> f64 {
    let mut t = 0.0;
    while t < max_secs {
        if done(&car.state) {
            return t;
        }
        let c = controls(&car.state, t);
        car.step(c, DT);
        t += DT;
        assert!(car.state.velocity.length().is_finite(), "physics blew up at t={t}");
    }
    t
}

fn full_throttle(_: &CarState, _: f64) -> Controls {
    Controls { throttle: 1.0, ..Default::default() }
}

#[test]
fn stays_at_rest_with_no_input() {
    let mut car = car_at_rest();
    drive(&mut car, 10.0, |_, _| Controls::default());
    assert!(car.state.position.length() < 1e-3, "crept {:?}", car.state.position);
    assert!(car.state.speed() < 1e-3);
    assert!(car.state.yaw_rate.abs() < 1e-6);
    let idle = car.params.engine.idle_rpm;
    assert!((car.state.engine_rpm() - idle).abs() < 300.0, "rpm {}", car.state.engine_rpm());
}

#[test]
fn zero_to_hundred_is_plausible_and_straight() {
    let mut car = car_at_rest();
    let t = drive_until(&mut car, 20.0, &mut full_throttle, |s| s.speed() >= 100.0 * KMH);
    println!("0-100 km/h: {t:.2} s");
    assert!((3.5..9.0).contains(&t), "0-100 took {t:.2} s");
    assert!(car.state.position.y.abs() < 0.05, "pulled sideways {}", car.state.position.y);
    assert!(car.state.heading.abs() < 1e-3, "turned {}", car.state.heading);
}

#[test]
fn top_speed_is_capped_by_rev_limiter() {
    let mut car = car_at_rest();
    drive(&mut car, 60.0, full_throttle);
    let kmh = car.state.speed() / KMH;
    println!("top speed: {kmh:.0} km/h, {:.0} rpm", car.state.engine_rpm());
    assert!((100.0..200.0).contains(&kmh), "top speed {kmh:.0} km/h");
    assert!(car.state.engine_rpm() < car.params.engine.rev_limit_rpm + 200.0);
}

#[test]
fn full_brake_from_hundred_stops_in_plausible_distance() {
    let mut car = car_rolling(100.0 * KMH);
    let mut brake = |_: &CarState, _: f64| Controls { brake: 1.0, ..Default::default() };
    let t = drive_until(&mut car, 10.0, &mut brake, |s| s.speed() < 0.1);
    let distance = car.state.position.x;
    println!("100-0 km/h: {distance:.1} m in {t:.2} s");
    assert!((30.0..60.0).contains(&distance), "stopped in {distance:.1} m");
    assert!(car.state.position.y.abs() < 0.5);
}

#[test]
fn coasting_never_gains_speed() {
    let mut car = car_rolling(80.0 * KMH);
    let mut previous = car.state.speed();
    drive(&mut car, 20.0, |s, _| {
        assert!(s.speed() <= previous + 1e-9, "gained speed: {} -> {}", previous, s.speed());
        previous = s.speed();
        Controls { clutch: 1.0, ..Default::default() }
    });
}

#[test]
fn skidpad_lateral_grip_is_plausible() {
    // Hold ~60 km/h and wind the steering in slowly; record the best sustained
    // lateral acceleration before the car lets go.
    let mut car = car_rolling(60.0 * KMH);
    let target = 60.0 * KMH;
    let mut best: f64 = 0.0;
    let mut smoothed: f64 = 0.0;
    drive(&mut car, 12.0, |s, t| {
        smoothed += (s.accel.y.abs() - smoothed) * 0.05;
        if s.body_slip_angle().abs() < 10f64.to_radians() {
            best = best.max(smoothed);
        }
        let speed_error = target - s.speed();
        Controls {
            steer: (t / 12.0) * 0.25,
            throttle: (speed_error * 0.5).clamp(0.0, 1.0),
            brake: (-speed_error * 0.2).clamp(0.0, 1.0),
            ..Default::default()
        }
    });
    let g = best / GRAVITY;
    println!("skidpad: {g:.2} g");
    assert!((0.8..1.25).contains(&g), "peak lateral {g:.2} g");
}

#[test]
fn handbrake_turn_rotates_the_car() {
    let mut car = car_rolling(50.0 * KMH);
    let mut peak_slip: f64 = 0.0;
    drive(&mut car, 2.0, |s, t| {
        peak_slip = peak_slip.max(s.body_slip_angle().abs());
        Controls {
            steer: 0.3,
            handbrake: if t < 0.8 { 1.0 } else { 0.0 },
            clutch: if t < 0.8 { 1.0 } else { 0.0 },
            ..Default::default()
        }
    });
    let deg = rad_to_deg(peak_slip);
    println!("handbrake turn: peak drift angle {deg:.0} deg");
    assert!(deg > 25.0, "only reached {deg:.0} deg");
}

#[test]
fn handbrake_locks_rears_without_stalling() {
    let mut car = car_rolling(40.0 * KMH);
    drive(&mut car, 0.5, |_, _| Controls { handbrake: 1.0, ..Default::default() });
    assert!(car.state.wheels[RL].omega.abs() < 0.5, "rears still spinning {}", car.state.wheels[RL].omega);
    let idle = car.params.engine.idle_rpm;
    assert!(car.state.engine_rpm() > idle * 0.7, "engine dragged to {:.0} rpm", car.state.engine_rpm());
}

/// Peak rear slip ratio over a short window at 40 km/h with full throttle,
/// with or without a clutch kick at the start.
fn rear_slip_after(kick: bool) -> f64 {
    let mut car = car_rolling(40.0 * KMH);
    let mut peak: f64 = 0.0;
    drive(&mut car, 1.2, |s, t| {
        if t > 0.35 {
            peak = peak.max(s.wheels[RL].slip_ratio.max(s.wheels[RR].slip_ratio));
        }
        Controls { throttle: 1.0, clutch: if kick && t < 0.35 { 1.0 } else { 0.0 }, ..Default::default() }
    });
    peak
}

#[test]
fn clutch_kick_breaks_rear_traction() {
    let plain = rear_slip_after(false);
    let kicked = rear_slip_after(true);
    println!("rear slip ratio: plain {plain:.2}, clutch kick {kicked:.2}");
    assert!(kicked > 0.3, "clutch kick only reached slip ratio {kicked:.2}");
    assert!(kicked > plain * 1.5, "kick {kicked:.2} vs plain {plain:.2}");
}

#[test]
fn steering_is_mirror_symmetric() {
    let run = |steer: f64| {
        let mut car = car_rolling(60.0 * KMH);
        drive(&mut car, 3.0, |_, _| Controls { steer, throttle: 0.6, ..Default::default() });
        car.state
    };
    let left = run(0.4);
    let right = run(-0.4);
    assert!((left.position.x - right.position.x).abs() < 1e-6);
    assert!((left.position.y + right.position.y).abs() < 1e-6);
    assert!((left.heading + right.heading).abs() < 1e-9);
}

#[test]
fn simulation_is_deterministic() {
    let run = || {
        let mut car = car_at_rest();
        drive(&mut car, 5.0, |_, t| Controls {
            steer: (t * 1.7).sin(),
            throttle: 0.5 + 0.5 * (t * 0.9).cos(),
            handbrake: if (t as i64) % 2 == 0 { 0.0 } else { 0.4 },
            ..Default::default()
        });
        car.state
    };
    let (a, b) = (run(), run());
    assert_eq!(a.position, b.position);
    assert_eq!(a.heading.to_bits(), b.heading.to_bits());
}

#[test]
fn can_hold_a_countersteered_drift() {
    // Kick into a left-hand drift, then hold it with a simple driver: point the
    // fronts along the direction of travel (counter-steer) and balance the
    // angle with throttle.
    let mut car = car_rolling(45.0 * KMH);
    let target = -25f64.to_radians();
    let max_lock = car.params.steering.max_lock();
    let mut angles = vec![];
    drive(&mut car, 8.0, |s, t| {
        let beta = s.body_slip_angle();
        if t < 0.5 {
            return Controls { steer: 0.4, throttle: 1.0, clutch: if t < 0.3 { 1.0 } else { 0.0 }, ..Default::default() };
        }
        if t > 2.0 {
            angles.push(beta);
            assert!(s.yaw_rate > 0.0, "stopped rotating left at t={t:.2}");
            assert!(s.steer_angle < 0.0, "not counter-steering at t={t:.2}");
        }
        let steer = beta + (beta - target) - 0.1 * s.yaw_rate;
        Controls { steer: steer / max_lock, throttle: (0.6 + (beta - target)).clamp(0.0, 1.0), ..Default::default() }
    });
    let (min, max) = angles.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &b| (lo.min(b), hi.max(b)));
    println!("held drift angle between {:.0} and {:.0} deg", rad_to_deg(max), rad_to_deg(min));
    assert!(rad_to_deg(max) < -15.0 && rad_to_deg(min) > -45.0);
}
