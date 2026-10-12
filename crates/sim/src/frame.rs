//! Replay state frames: one recorded moment of a run, holding everything the
//! viewer draws (the car, its smoke and skids, the HUD).
//!
//! Frames are stored as named numbers. A replay written before a field
//! existed still loads (the field reads as 0), and one from before a field
//! was removed just has a column nobody reads.

use common::Vec2;
use common::math::{angle_diff, wrap_angle};

use crate::Controls;
use crate::World;
use crate::car::drivetrain::DrivetrainState;
use crate::car::{CarState, WheelState};
use crate::scoring::{Breakdown, Scorer};

const WHEELS: [&str; 4] = ["fl", "fr", "rl", "rr"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub position: Vec2,
    pub heading: f64,
    pub velocity: Vec2,
    pub yaw_rate: f64,
    pub steer_angle: f64,
    pub accel: Vec2,
    pub wheels: [WheelState; 4],
    pub engine_omega: f64,
    pub clutch_temp: f64,
    pub clutch_locked: bool,
    pub limiter_active: bool,
    pub engine_running: bool,
    pub controls: Controls,
    /// Metres along the track since the start.
    pub progress: f64,
    /// Banked score so far.
    pub score: Breakdown,
    pub combo_active: bool,
    pub combo_pending: Breakdown,
    pub combo_multiplier: f64,
    pub combo_taps: u32,
}

impl Frame {
    /// The car and score as they are now.
    pub fn capture(world: &World, scorer: &Scorer) -> Self {
        let s = &world.car.state;
        let d = &s.drivetrain;
        let combo = &scorer.combo;
        Self {
            position: s.position,
            heading: s.heading,
            velocity: s.velocity,
            yaw_rate: s.yaw_rate,
            steer_angle: s.steer_angle,
            accel: s.accel,
            wheels: s.wheels,
            engine_omega: d.engine_omega,
            clutch_temp: d.clutch_temp,
            clutch_locked: d.clutch_locked,
            limiter_active: d.limiter_active,
            engine_running: d.engine_running,
            controls: s.controls,
            progress: world.progress,
            score: scorer.total,
            combo_active: combo.active,
            combo_pending: combo.pending,
            combo_multiplier: combo.multiplier,
            combo_taps: combo.taps,
        }
    }

    /// A car state to draw. Suspension isn't recorded (nothing draws it).
    pub fn car_state(&self) -> CarState {
        CarState {
            position: self.position,
            heading: self.heading,
            velocity: self.velocity,
            yaw_rate: self.yaw_rate,
            steer_angle: self.steer_angle,
            wheels: self.wheels,
            drivetrain: DrivetrainState {
                engine_omega: self.engine_omega,
                clutch_locked: self.clutch_locked,
                limiter_active: self.limiter_active,
                engine_running: self.engine_running,
                clutch_temp: self.clutch_temp,
                below_stall: 0.0,
            },
            accel: self.accel,
            controls: self.controls,
            ..CarState::default()
        }
    }

    /// Put the recorded score into a scorer (for the score HUD).
    pub fn apply_score(&self, scorer: &mut Scorer) {
        scorer.total = self.score;
        let combo = &mut scorer.combo;
        combo.active = self.combo_active;
        combo.pending = self.combo_pending;
        combo.multiplier = self.combo_multiplier;
        combo.taps = self.combo_taps;
    }

    /// Part way (`t`, 0..1) from this frame to `next`. Switches (clutch
    /// locked, combo active, ...) flip half way.
    pub fn lerp(&self, next: &Frame, t: f64) -> Frame {
        let to = next.values();
        let mut out = *self;
        let mut k = 0;
        out.visit(&mut |_, _, v| {
            *v += (to[k] - *v) * t;
            k += 1;
        });
        out.heading = wrap_angle(self.heading + angle_diff(self.heading, next.heading) * t);
        out.combo_taps = if t < 0.5 { self.combo_taps } else { next.combo_taps };
        out
    }

    /// Field names, in storage order (e.g. "x", "rl_slip_speed").
    pub fn names() -> Vec<String> {
        let mut names = vec![];
        Frame::default().visit(&mut |prefix, name, _| {
            names.push(if prefix.is_empty() { name.to_owned() } else { format!("{prefix}_{name}") });
        });
        names
    }

    /// Every field's value, in storage order.
    pub fn values(&self) -> Vec<f64> {
        let mut values = vec![];
        let mut copy = *self;
        copy.visit(&mut |_, _, v| values.push(*v));
        values
    }

    /// Build a frame from values in storage order (missing ones are 0).
    pub fn from_values(values: impl IntoIterator<Item = f64>) -> Frame {
        let mut values = values.into_iter();
        let mut frame = Frame::default();
        frame.visit(&mut |_, _, v| *v = values.next().unwrap_or(0.0));
        frame
    }

    /// Hand every stored number to `f` as (prefix, name, value), in storage
    /// order. Reading, writing and interpolating all go through here, so they
    /// can't disagree about the layout. Flags and counts pass through as f64.
    fn visit(&mut self, f: &mut dyn FnMut(&'static str, &'static str, &mut f64)) {
        vec2(f, "x", "y", &mut self.position);
        f("", "heading", &mut self.heading);
        vec2(f, "vx", "vy", &mut self.velocity);
        f("", "yaw_rate", &mut self.yaw_rate);
        f("", "steer_angle", &mut self.steer_angle);
        vec2(f, "accel_x", "accel_y", &mut self.accel);
        for (prefix, w) in WHEELS.into_iter().zip(&mut self.wheels) {
            f(prefix, "omega", &mut w.omega);
            f(prefix, "load", &mut w.load);
            f(prefix, "force_x", &mut w.force.x);
            f(prefix, "force_y", &mut w.force.y);
            f(prefix, "slip_ratio", &mut w.slip_ratio);
            f(prefix, "slip_angle", &mut w.slip_angle);
            f(prefix, "combined_slip", &mut w.combined_slip);
            f(prefix, "slip_speed", &mut w.slip_speed);
        }
        f("", "engine_omega", &mut self.engine_omega);
        f("", "clutch_temp", &mut self.clutch_temp);
        flag(f, "clutch_locked", &mut self.clutch_locked);
        flag(f, "limiter_active", &mut self.limiter_active);
        flag(f, "engine_running", &mut self.engine_running);
        let c = &mut self.controls;
        f("", "steer", &mut c.steer);
        f("", "throttle", &mut c.throttle);
        f("", "brake", &mut c.brake);
        f("", "handbrake", &mut c.handbrake);
        f("", "clutch", &mut c.clutch);
        f("", "progress", &mut self.progress);
        breakdown(f, "score", &mut self.score);
        flag(f, "combo_active", &mut self.combo_active);
        breakdown(f, "combo", &mut self.combo_pending);
        f("", "combo_multiplier", &mut self.combo_multiplier);
        let mut taps = f64::from(self.combo_taps);
        f("", "combo_taps", &mut taps);
        self.combo_taps = taps.round().max(0.0) as u32;
    }
}

fn vec2(f: &mut dyn FnMut(&'static str, &'static str, &mut f64), x: &'static str, y: &'static str, v: &mut Vec2) {
    f("", x, &mut v.x);
    f("", y, &mut v.y);
}

fn flag(f: &mut dyn FnMut(&'static str, &'static str, &mut f64), name: &'static str, value: &mut bool) {
    let mut v = if *value { 1.0 } else { 0.0 };
    f("", name, &mut v);
    *value = v >= 0.5;
}

fn breakdown(f: &mut dyn FnMut(&'static str, &'static str, &mut f64), prefix: &'static str, b: &mut Breakdown) {
    f(prefix, "line", &mut b.line);
    f(prefix, "angle", &mut b.angle);
    f(prefix, "style", &mut b.style);
    f(prefix, "penalty", &mut b.penalty);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        let mut f = Frame {
            position: Vec2::new(10.0, -4.0),
            heading: 3.0,
            clutch_locked: true,
            engine_running: true,
            combo_taps: 2,
            progress: 120.0,
            ..Frame::default()
        };
        f.wheels[2].slip_speed = 7.5;
        f.score.angle = 300.0;
        f
    }

    #[test]
    fn values_round_trip_by_name() {
        let f = frame();
        assert_eq!(Frame::names().len(), f.values().len());
        assert_eq!(Frame::from_values(f.values()), f);
        assert!(Frame::names().contains(&"rl_slip_speed".to_owned()));
        // Fewer values than fields (an older file): the rest are 0.
        let short = Frame::from_values(f.values().into_iter().take(3));
        assert_eq!((short.position, short.heading, short.progress), (f.position, f.heading, 0.0));
    }

    #[test]
    fn lerp_takes_the_short_way_round_and_flips_switches_half_way() {
        let a = frame();
        let b = Frame { heading: -3.0, clutch_locked: false, progress: 130.0, combo_taps: 3, ..a };
        let mid = a.lerp(&b, 0.25);
        assert!((mid.progress - 122.5).abs() < 1e-9);
        // 3.0 to -3.0 rad is a short hop across +-pi, not a spin the long way.
        assert!(mid.heading.abs() > 3.0);
        assert!(mid.clutch_locked && mid.combo_taps == 2);
        let late = a.lerp(&b, 0.75);
        assert!(!late.clutch_locked && late.combo_taps == 3);
    }
}
