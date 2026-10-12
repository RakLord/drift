//! Observations and episodes.

mod support;

use std::collections::HashSet;

use common::Rng;
use sim::car::CarParams;
use sim::episode::{EndReason, Episode, EpisodeConfig};
use sim::scoring::ScoringParams;
use sim::sensors::observation::{INPUTS, Observation, label};
use sim::track::EdgeKind;
use sim::{Controller, Controls};
use support::oval;

fn config() -> EpisodeConfig {
    EpisodeConfig {
        max_secs: 30.0,
        start_speed_kmh: 50.0,
        decision_every: 4,
        sensor_noise: 0.0,
        no_progress_secs: 4.0,
        max_backwards: 20.0,
        max_hard_hits: 3,
        end_on_spin: true,
    }
}

fn episode(seed: u64) -> Episode {
    let track = oval(EdgeKind::Hard, EdgeKind::Soft, vec![]);
    Episode::new(track, CarParams::default(), ScoringParams::default(), config(), seed)
}

/// Steers using only its inputs: towards the track direction and the centre.
struct Follower {
    throttle: f32,
}

impl Controller for Follower {
    fn decide(&mut self, obs: &Observation) -> Controls {
        let t = sim::sensors::observation::offset("track");
        let (heading_sin, offset, curvature) = (obs.values[t], obs.values[t + 2], obs.values[t + 3]);
        let steer = (0.1 * curvature - 2.0 * heading_sin - 1.5 * offset).clamp(-1.0, 1.0);
        Controls { steer: f64::from(steer), throttle: f64::from(self.throttle), ..Default::default() }
    }
}

/// Random controls, fixed per seed.
struct Random(Rng);

impl Controller for Random {
    fn decide(&mut self, _: &Observation) -> Controls {
        let r = &mut self.0;
        Controls {
            steer: r.range(-1.0, 1.0),
            throttle: r.next_f64(),
            brake: if r.chance(0.1) { 1.0 } else { 0.0 },
            handbrake: if r.chance(0.1) { 1.0 } else { 0.0 },
            clutch: if r.chance(0.1) { 1.0 } else { 0.0 },
        }
    }
}

#[test]
fn layout_has_111_uniquely_labelled_inputs() {
    assert_eq!(INPUTS, 111);
    let labels: HashSet<String> = (0..INPUTS).map(label).collect();
    assert_eq!(labels.len(), INPUTS);
}

#[test]
fn inputs_are_scaled_sensibly() {
    // Pinned at +-1 most of the time means the scaling is wrong.
    let mut pinned = [0usize; INPUTS];
    let mut samples = 0;
    for seed in 0..4 {
        let mut ep = episode(seed);
        let mut driver = Random(Rng::new(seed));
        while ep.step(&mut driver).is_none() {
            if ep.world.car.state.speed() > 3.0 {
                let obs = ep.observe();
                for (p, v) in pinned.iter_mut().zip(obs.values) {
                    *p += usize::from(v.abs() >= 0.999);
                }
                samples += 1;
            }
        }
    }
    assert!(samples > 200, "only {samples} samples");
    // Skip inputs where +-1 is a normal reading: rays (nothing in range),
    // on/off flags and pedals, cosines of small angles, "no clip" (-1), and
    // corner distance (capped at "far away"; this oval's straights are long).
    let normal_at_limit = |name: &str| {
        ["wall", "clip at", "progress", "distance", "clutch temp"].iter().any(|k| name.contains(k))
            || ["drifting", "in zone", "throttle", "brake", "handbrake", "clutch", "drift cos", "heading cos"].contains(&name)
    };
    let continuous = (48..INPUTS).filter(|&i| !normal_at_limit(&label(i)));
    for i in continuous {
        let share = pinned[i] as f64 / samples as f64;
        assert!(share < 0.25, "{} is pinned at +-1 {:.0}% of the time", label(i), share * 100.0);
    }
}

#[test]
fn episodes_are_deterministic() {
    let run = || episode(7).run(&mut Random(Rng::new(3)));
    assert_eq!(run(), run());
}

#[test]
fn a_simple_driver_laps_until_time_is_up() {
    let result = episode(1).run(&mut Follower { throttle: 0.2 });
    println!("{result:?}");
    assert_eq!(result.end, EndReason::TimeUp);
    assert!(result.laps() > 0.5, "only {:.2} laps", result.laps());
}

#[test]
fn stopping_ends_the_run_for_no_progress() {
    struct Stop;
    impl Controller for Stop {
        fn decide(&mut self, _: &Observation) -> Controls {
            Controls { brake: 1.0, clutch: 1.0, ..Default::default() }
        }
    }
    let result = episode(1).run(&mut Stop);
    assert_eq!(result.end, EndReason::NoProgress);
    assert!(result.time < 10.0);
}
