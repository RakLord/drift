//! The network's inputs: one place that defines what every input is, in
//! which order, and how it is scaled (roughly -1..1). Everything is relative
//! to the car, so a network can't memorise a particular track.

use common::Rng;

use super::driver_prep::{self, CORNERS_AHEAD, FEATURES_PER_CORNER};
use super::rays::{self, RAY_COUNT, RAY_RANGE};
use crate::GRAVITY;
use crate::World;
use crate::scoring::Scorer;
use crate::track::Side;

/// A named run of inputs.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub name: &'static str,
    pub len: usize,
    /// Label for each input in the group.
    pub labels: fn(usize) -> String,
}

const WHEEL_NAMES: [&str; 4] = ["FL", "FR", "RL", "RR"];
const WHEEL_VALUES: [&str; 4] = ["slip angle", "slip ratio", "grip use", "load"];
const BODY: [&str; 8] = ["fwd vel", "side vel", "speed", "drift sin", "drift cos", "yaw rate", "fwd g", "side g"];
const CAR: [&str; 7] = ["steer", "throttle", "brake", "handbrake", "clutch", "rpm", "clutch temp"];
const TRACK: [&str; 6] = ["heading sin", "heading cos", "offset", "curvature", "left wall", "right wall"];
const PREP: [&str; FEATURES_PER_CORNER] =
    ["distance", "progress", "angle", "sharpness", "clip at", "zone", "wall in", "wall out"];
const DRIFT: [&str; 2] = ["drifting", "in zone"];

/// The input layout, in order. Changing it changes `LAYOUT_HASH`, which
/// makes old genome files refuse to load.
pub const LAYOUT: [Group; 7] = [
    Group { name: "rays", len: RAY_COUNT, labels: |i| format!("ray {i}") },
    Group { name: "body", len: 8, labels: |i| BODY[i].to_owned() },
    Group { name: "wheels", len: 16, labels: |i| format!("{} {}", WHEEL_NAMES[i / 4], WHEEL_VALUES[i % 4]) },
    Group { name: "car", len: 7, labels: |i| CAR[i].to_owned() },
    Group { name: "track", len: 6, labels: |i| TRACK[i].to_owned() },
    Group {
        name: "driver prep",
        len: CORNERS_AHEAD * FEATURES_PER_CORNER,
        labels: |i| format!("corner {} {}", i / FEATURES_PER_CORNER + 1, PREP[i % FEATURES_PER_CORNER]),
    },
    Group { name: "drift", len: 2, labels: |i| DRIFT[i].to_owned() },
];

pub const INPUTS: usize = {
    let mut total = 0;
    let mut i = 0;
    while i < LAYOUT.len() {
        total += LAYOUT[i].len;
        i += 1;
    }
    total
};

/// Identifies the layout (names and sizes). Bump `LAYOUT_VERSION` whenever
/// the meaning or scaling of an input changes without its size changing.
pub const LAYOUT_VERSION: u32 = 1;

pub fn layout_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ u64::from(LAYOUT_VERSION);
    for g in LAYOUT {
        for b in g.name.bytes().chain((g.len as u32).to_le_bytes()) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// Index of the first input in the named group.
pub fn offset(group: &str) -> usize {
    let mut start = 0;
    for g in LAYOUT {
        if g.name == group {
            return start;
        }
        start += g.len;
    }
    panic!("no input group named {group}");
}

/// Label of input `index` (e.g. "FL slip ratio").
pub fn label(index: usize) -> String {
    let mut start = 0;
    for g in LAYOUT {
        if index < start + g.len {
            return (g.labels)(index - start);
        }
        start += g.len;
    }
    format!("input {index}")
}

#[derive(Clone, Debug)]
pub struct Observation {
    pub values: [f32; INPUTS],
}

/// Builds observations, keeping the ray angles and a noise generator.
pub struct Observer {
    angles: [f64; RAY_COUNT],
    noise: f64,
    rng: Rng,
}

/// Normalisation references.
const SPEED_REF: f64 = 50.0;
const SIDE_SPEED_REF: f64 = 20.0;
const YAW_REF: f64 = 2.0;
const G_REF: f64 = 1.5 * GRAVITY;
const CURVATURE_REF: f64 = 30.0;

impl Observer {
    /// `noise` is the std dev of Gaussian noise added to every input
    /// (0 = exact); `seed` makes it reproducible.
    pub fn new(noise: f64, seed: u64) -> Self {
        Self { angles: rays::angles(), noise, rng: Rng::with_stream(seed, 0x0b5e) }
    }

    pub fn observe(&mut self, world: &World, scorer: &Scorer) -> Observation {
        let mut v = Vec::with_capacity(INPUTS);
        let car = &world.car;
        let s = &car.state;
        let track = &world.track;

        for d in rays::cast(track, s.position, s.heading, &self.angles) {
            v.push(1.0 - d / RAY_RANGE);
        }

        let local = s.local_velocity();
        let beta = s.body_slip_angle();
        v.extend([
            local.x / SPEED_REF,
            local.y / SIDE_SPEED_REF,
            s.speed() / SPEED_REF,
            beta.sin(),
            beta.cos(),
            (s.yaw_rate / YAW_REF).tanh(),
            s.accel.x / G_REF,
            s.accel.y / G_REF,
        ]);

        let static_loads = car.params.body.static_loads();
        for (w, static_load) in s.wheels.iter().zip(static_loads) {
            v.extend([
                w.slip_angle / std::f64::consts::FRAC_PI_2,
                w.slip_ratio.tanh(),
                (w.combined_slip / 3.0).min(1.0),
                w.load / static_load - 1.0,
            ]);
        }

        let c = &s.controls;
        v.extend([
            s.steer_angle / car.params.steering.max_lock(),
            c.throttle,
            c.brake,
            c.handbrake,
            c.clutch,
            s.engine_rpm() / car.params.engine.rev_limit_rpm,
            // 0 = cool, 1 = fully faded.
            (s.drivetrain.clutch_temp - car.params.clutch.ambient_temp)
                / (car.params.clutch.fade_full_temp - car.params.clutch.ambient_temp),
        ]);

        let place = &world.place;
        let heading_error = track.heading_error(place.index, s.heading);
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        v.extend([
            heading_error.sin(),
            heading_error.cos(),
            // Out to the back of the verge, so on-the-grass still reads.
            place.offset / (track.half_width() + track.def.verge),
            track.samples[place.index].curvature * CURVATURE_REF,
            flag(track.edge(place.index, Side::Left) == crate::track::EdgeKind::Hard),
            flag(track.edge(place.index, Side::Right) == crate::track::EdgeKind::Hard),
        ]);

        v.extend(driver_prep::features(&driver_prep::upcoming(track, place.distance)));
        v.extend([flag(scorer.drift.active), flag(scorer.zone_lit.iter().any(|&lit| lit))]);

        debug_assert_eq!(v.len(), INPUTS);
        let mut values = [0.0f32; INPUTS];
        for (out, x) in values.iter_mut().zip(v) {
            let noisy = if self.noise > 0.0 { x + self.rng.gaussian() * self.noise } else { x };
            *out = noisy.clamp(-1.0, 1.0) as f32;
        }
        Observation { values }
    }
}
