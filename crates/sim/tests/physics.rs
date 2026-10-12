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

/// Pull away like a driver: clutch in while the revs build, ease it out over
/// most of a second, then flat out.
fn launch(_: &CarState, t: f64) -> Controls {
    let clutch = 1.0 - ((t - 0.4) / 0.8).clamp(0.0, 1.0);
    Controls { throttle: 1.0, clutch, ..Default::default() }
}

#[test]
fn idles_at_rest_with_the_clutch_in() {
    let mut car = car_at_rest();
    drive(&mut car, 10.0, |_, _| Controls { clutch: 1.0, ..Default::default() });
    assert!(car.state.position.length() < 1e-3, "crept {:?}", car.state.position);
    assert!(car.state.speed() < 1e-3);
    assert!(car.state.yaw_rate.abs() < 1e-6);
    assert!(car.state.engine_running());
    let idle = car.params.engine.idle_rpm;
    assert!((car.state.engine_rpm() - idle).abs() < 300.0, "rpm {}", car.state.engine_rpm());
}

#[test]
fn stalls_at_rest_with_the_clutch_out() {
    let mut car = car_at_rest();
    drive(&mut car, 2.0, |_, _| Controls::default());
    assert!(!car.state.engine_running(), "should have stalled");
    // It lurches forward a little before dying, like a real car.
    assert!(car.state.speed() < 2.0, "lurched to {:.2} m/s", car.state.speed());

    // Clutch in and turn the key: back to idle.
    car.restart_engine();
    drive(&mut car, 1.0, |_, _| Controls { clutch: 1.0, ..Default::default() });
    assert!(car.state.engine_running());
}

#[test]
fn dropping_the_clutch_at_idle_stalls_but_a_proper_launch_does_not() {
    let mut dropped = car_at_rest();
    drive(&mut dropped, 2.0, |_, _| Controls { throttle: 0.3, ..Default::default() });
    assert!(!dropped.state.engine_running());

    let mut launched = car_at_rest();
    drive(&mut launched, 3.0, launch);
    assert!(launched.state.engine_running());
    assert!(launched.state.speed() > 10.0);
}

#[test]
fn zero_to_hundred_is_plausible_and_straight() {
    let mut car = car_at_rest();
    let t = drive_until(&mut car, 20.0, &mut launch, |s| s.speed() >= 100.0 * KMH);
    println!("0-100 km/h: {t:.2} s");
    assert!((3.5..9.0).contains(&t), "0-100 took {t:.2} s");
    assert!(car.state.position.y.abs() < 0.05, "pulled sideways {}", car.state.position.y);
    assert!(car.state.heading.abs() < 1e-3, "turned {}", car.state.heading);
}

#[test]
fn top_speed_is_capped_by_rev_limiter() {
    let mut car = car_at_rest();
    drive(&mut car, 60.0, launch);
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
fn handbrake_with_the_clutch_in_locks_the_rears_and_spares_the_engine() {
    let mut car = car_rolling(40.0 * KMH);
    drive(&mut car, 0.5, |_, _| Controls { handbrake: 1.0, clutch: 1.0, ..Default::default() });
    assert!(car.state.wheels[RL].omega.abs() < 0.5, "rears still spinning {}", car.state.wheels[RL].omega);
    assert!(car.state.engine_running());
    let idle = car.params.engine.idle_rpm;
    assert!(car.state.engine_rpm() > idle * 0.7, "engine dragged to {:.0} rpm", car.state.engine_rpm());
}

#[test]
fn a_quick_handbrake_yank_in_gear_does_not_stall() {
    // In gear the engine's inertia (times the ratio squared) is on the rear
    // axle, so a short pull drops the revs but they recover.
    let mut car = car_rolling(60.0 * KMH);
    let mut lowest: f64 = f64::MAX;
    drive(&mut car, 2.0, |s, t| {
        lowest = lowest.min(s.engine_rpm());
        Controls { throttle: 0.5, handbrake: if t < 0.2 { 1.0 } else { 0.0 }, ..Default::default() }
    });
    println!("quick yank in gear: revs dipped to {lowest:.0} rpm");
    assert!(car.state.engine_running(), "stalled (revs dipped to {lowest:.0})");
}

#[test]
fn holding_the_handbrake_in_gear_stalls() {
    let mut car = car_rolling(40.0 * KMH);
    let mut stalled_at = None;
    drive(&mut car, 3.0, |s, t| {
        if stalled_at.is_none() && !s.engine_running() {
            stalled_at = Some(t);
        }
        Controls { handbrake: 1.0, ..Default::default() }
    });
    println!("held handbrake in gear: stalled after {stalled_at:?} s");
    assert!(stalled_at.is_some(), "never stalled");
}

#[test]
fn riding_the_clutch_cooks_it_and_it_fades() {
    // Revs pinned, clutch slipping, speed held down (here by the brakes):
    // the way the AI was riding it.
    let mut car = car_at_rest();
    drive(&mut car, 15.0, |_, t| Controls {
        throttle: 1.0,
        clutch: if t < 0.5 { 1.0 } else { 0.85 },
        brake: 1.0,
        ..Default::default()
    });
    let p = &car.params.clutch;
    let temp = car.state.drivetrain.clutch_temp;
    println!("after 15 s riding the clutch: {temp:.0} degC");
    assert!(temp > p.fade_start_temp, "only {temp:.0} degC");
    assert!(sim::car::engine::clutch_fade(p, temp) < 1.0);
}

#[test]
fn a_clutch_kick_barely_warms_it_and_it_cools_off() {
    let mut car = car_rolling(40.0 * KMH);
    let start = car.state.drivetrain.clutch_temp;
    let mut peak: f64 = 0.0;
    drive(&mut car, 1.5, |s, t| {
        peak = peak.max(s.drivetrain.clutch_temp);
        Controls { throttle: 1.0, clutch: if t < 0.3 { 1.0 } else { 0.0 }, ..Default::default() }
    });
    println!("clutch kick: {start:.0} -> {peak:.0} degC");
    assert!(peak - start < 40.0, "a single kick heated it {:.0} degC", peak - start);

    // Heat it up, then let it cool with the clutch in.
    car.state.drivetrain.clutch_temp = 400.0;
    drive(&mut car, 120.0, |_, _| Controls { clutch: 1.0, ..Default::default() });
    assert!(car.state.drivetrain.clutch_temp < 120.0, "still {:.0} degC", car.state.drivetrain.clutch_temp);
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
