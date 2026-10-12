//! One run: a controller drives one car around one track from a rolling
//! start until time runs out or the run fails. Deterministic given the seed.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::Controls;
use crate::World;
use crate::car::{CarParams, CarState};
use crate::controller::Controller;
use crate::scoring::{Breakdown, ScoreEventKind, Scorer, ScoringParams};
use crate::sensors::observation::{Observation, Observer};
use crate::track::Track;

pub const TICK: f64 = 1.0 / 120.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeConfig {
    pub max_secs: f64,
    /// Speed at the start line, km/h (Formula Drift style rolling start).
    pub start_speed_kmh: f64,
    /// The controller decides every this many ticks; controls are held in
    /// between.
    pub decision_every: u32,
    /// Std dev of noise added to every input (0 = exact).
    pub sensor_noise: f64,
    /// Ends the run if the car hasn't got further round the track for this
    /// long, seconds.
    pub no_progress_secs: f64,
    /// Ends the run if the car falls this far behind its best progress, m.
    pub max_backwards: f64,
    /// Ends the run on this many hard wall hits.
    pub max_hard_hits: u32,
    pub end_on_spin: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    TimeUp,
    NoProgress,
    Backwards,
    Crashed,
    Spun,
}

impl EndReason {
    pub const ALL: [Self; 5] = [Self::TimeUp, Self::NoProgress, Self::Backwards, Self::Crashed, Self::Spun];

    pub fn name(self) -> &'static str {
        match self {
            Self::TimeUp => "time up",
            Self::NoProgress => "no progress",
            Self::Backwards => "backwards",
            Self::Crashed => "crashed",
            Self::Spun => "spun",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EpisodeStats {
    /// Seconds spent drifting.
    pub drift_time: f64,
    /// Sum of |drift angle| (deg) x seconds while drifting, for the average.
    pub angle_seconds: f64,
    pub zones_entered: u32,
    pub clips: u32,
    pub transitions: u32,
    pub wall_taps: u32,
    pub hard_hits: u32,
    pub spins: u32,
    pub ticks: u64,
}

impl EpisodeStats {
    /// Average drift angle while drifting, degrees.
    pub fn average_angle(&self) -> f64 {
        if self.drift_time > 0.0 { self.angle_seconds / self.drift_time } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeResult {
    pub score: Breakdown,
    /// Best distance reached along the track, metres (laps keep adding).
    pub progress: f64,
    pub track_length: f64,
    pub time: f64,
    pub end: EndReason,
    pub stats: EpisodeStats,
}

impl EpisodeResult {
    pub fn laps(&self) -> f64 {
        self.progress / self.track_length
    }
}

pub struct Episode {
    pub world: World,
    pub scorer: Scorer,
    config: EpisodeConfig,
    observer: Observer,
    held: Controls,
    /// Decisions happen when `tick % decision_every == offset`, so cars
    /// sharing a frame don't all think on the same tick.
    offset: u32,
    tick: u64,
    best_progress: f64,
    since_progress: f64,
    stats: EpisodeStats,
    end: Option<EndReason>,
}

impl Episode {
    pub fn new(track: Arc<Track>, car: CarParams, scoring: ScoringParams, config: EpisodeConfig, seed: u64) -> Self {
        let scorer = Scorer::new(scoring, &track);
        let mut world = World::new(track, car);
        let (position, heading) = world.track.start();
        let start = CarState::rolling(&world.car.params, position, heading, config.start_speed_kmh / 3.6);
        world.set_car_state(start);
        let offset = (seed % u64::from(config.decision_every.max(1))) as u32;
        Self {
            world,
            scorer,
            observer: Observer::new(config.sensor_noise, seed),
            config,
            held: Controls::default(),
            offset,
            tick: 0,
            best_progress: 0.0,
            since_progress: 0.0,
            stats: EpisodeStats::default(),
            end: None,
        }
    }

    pub fn ended(&self) -> Option<EndReason> {
        self.end
    }

    /// What the car senses right now.
    pub fn observe(&mut self) -> Observation {
        self.observer.observe(&self.world, &self.scorer)
    }

    /// Advance one tick, asking the controller for new controls on decision
    /// ticks. Returns why the run ended, once it has.
    pub fn step(&mut self, controller: &mut dyn Controller) -> Option<EndReason> {
        if self.end.is_some() {
            return self.end;
        }
        let every = u64::from(self.config.decision_every.max(1));
        if self.tick % every == u64::from(self.offset) % every {
            let observation = self.observe();
            self.held = controller.decide(&observation);
        }
        self.advance(self.held)
    }

    /// Advance one tick with the given controls (no controller involved).
    pub fn advance(&mut self, controls: Controls) -> Option<EndReason> {
        self.world.step(controls, TICK);
        self.tick += 1;
        let events = self.scorer.update(&self.world, self.world.contacts(), TICK);
        let stats = &mut self.stats;
        stats.ticks = self.tick;
        let mut spun = false;
        for e in events {
            match e.kind {
                ScoreEventKind::ZoneEnter => stats.zones_entered += 1,
                ScoreEventKind::Clip { .. } => stats.clips += 1,
                ScoreEventKind::Transition => stats.transitions += 1,
                ScoreEventKind::WallTap => stats.wall_taps += 1,
                ScoreEventKind::HardHit => stats.hard_hits += 1,
                ScoreEventKind::Spin => {
                    stats.spins += 1;
                    spun = true;
                }
                _ => {}
            }
        }
        if self.scorer.drift.active {
            stats.drift_time += TICK;
            stats.angle_seconds += self.world.car.state.body_slip_angle().abs().to_degrees() * TICK;
        }

        let progress = self.world.progress;
        if progress > self.best_progress + 1.0 {
            self.best_progress = progress;
            self.since_progress = 0.0;
        } else {
            self.since_progress += TICK;
        }

        let time = self.tick as f64 * TICK;
        self.end = if spun && self.config.end_on_spin {
            Some(EndReason::Spun)
        } else if self.stats.hard_hits >= self.config.max_hard_hits {
            Some(EndReason::Crashed)
        } else if progress < self.best_progress - self.config.max_backwards {
            Some(EndReason::Backwards)
        } else if self.since_progress >= self.config.no_progress_secs {
            Some(EndReason::NoProgress)
        } else if time >= self.config.max_secs {
            // A drift still going at the bell counts.
            self.scorer.finish();
            Some(EndReason::TimeUp)
        } else {
            None
        };
        self.end
    }

    /// Drive until the run ends.
    pub fn run(mut self, controller: &mut dyn Controller) -> EpisodeResult {
        controller.reset();
        while self.step(controller).is_none() {}
        self.result()
    }

    pub fn result(&self) -> EpisodeResult {
        EpisodeResult {
            score: self.scorer.total,
            progress: self.best_progress.max(self.world.progress),
            track_length: self.world.track.length,
            time: self.tick as f64 * TICK,
            end: self.end.unwrap_or(EndReason::TimeUp),
            stats: self.stats,
        }
    }
}
