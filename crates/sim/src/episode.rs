//! One run: a controller drives one car around one track from a rolling
//! start until time runs out or the run fails. Deterministic given the seed.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::Controls;
use crate::World;
use crate::car::{CarParams, CarState};
use crate::controller::Controller;
use crate::recording::{Recorder, Recording};
use crate::scoring::{Breakdown, ScoreEvent, ScoreEventKind, Scorer, ScoringParams};
use crate::sensors::observation::{Observation, Observer};
use crate::smoothness::{SawDetector, SmoothnessConfig};
use crate::track::Track;

pub const TICK: f64 = 1.0 / 120.0;
/// Below this speed (m/s) drift angles are noise and aren't counted.
const MOVING: f64 = 3.0;
/// Clutch held at this drift angle (degrees) or more counts in full in
/// `EpisodeStats::clutch_hold_angle`.
pub const CLUTCH_FULL_ANGLE_DEG: f64 = 45.0;

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
    /// A clutch press shorter than this is a kick; holding it longer counts
    /// towards `EpisodeStats::clutch_hold_time`, seconds.
    #[serde(default = "default_clutch_grace_secs")]
    pub clutch_grace_secs: f64,
    /// What counts as sawing the throttle or steering (see
    /// `EpisodeStats::throttle_saws`).
    #[serde(default)]
    pub smoothness: SmoothnessConfig,
    /// What a network's steer output means (see `SteeringOutput`).
    #[serde(default)]
    pub steering: SteeringOutput,
}

impl EpisodeConfig {
    /// Time between the controller's decisions, seconds.
    pub fn decision_secs(&self) -> f64 {
        f64::from(self.decision_every.max(1)) * TICK
    }
}

fn default_clutch_grace_secs() -> f64 {
    0.4
}

/// What a network driver's steer output means. Runs from before this
/// setting have none, and drive in `Position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SteeringOutput {
    /// Where the wheel should be, -1..1, fresh every decision.
    #[default]
    Position,
    /// How fast to turn the wheel: full output takes it from one lock to
    /// the other in `full_lock_secs`. Smooth by construction.
    Rate { full_lock_secs: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    TimeUp,
    NoProgress,
    Backwards,
    Crashed,
    Spun,
    Stalled,
}

impl EndReason {
    pub const ALL: [Self; 6] = [Self::TimeUp, Self::NoProgress, Self::Backwards, Self::Crashed, Self::Spun, Self::Stalled];

    pub fn name(self) -> &'static str {
        match self {
            Self::TimeUp => "time up",
            Self::NoProgress => "no progress",
            Self::Backwards => "backwards",
            Self::Crashed => "crashed",
            Self::Spun => "spun",
            Self::Stalled => "stalled",
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
    /// Seconds with the clutch pedal in the bite zone...
    pub clutch_pedal_time: f64,
    /// ...and of those, seconds actually slipping (riding it).
    pub clutch_slip_time: f64,
    /// Hottest the clutch got, degC.
    pub peak_clutch_temp: f64,
    /// Seconds with the handbrake and the clutch pedal past their free play.
    pub handbrake_time: f64,
    pub clutch_time: f64,
    /// Seconds with the clutch held past its free play for longer than a
    /// kick (`EpisodeConfig::clutch_grace_secs`)...
    #[serde(default)]
    pub clutch_hold_time: f64,
    /// ...and the same, each second weighted by drift angle (0 at no angle
    /// or standing still, 1 at `CLUTCH_FULL_ANGLE_DEG` and over).
    #[serde(default)]
    pub clutch_hold_angle: f64,
    /// Largest drift angle reached, degrees.
    pub max_angle: f64,
    /// Seconds spent above 20 degrees of drift angle (moving).
    pub above_20_time: f64,
    /// Flow credit x seconds while drifting (see `scoring::flow`): angle
    /// points' shape x how long each side was held.
    #[serde(default)]
    pub flow_seconds: f64,
    /// Sides drifted: starts, transitions and flicks.
    #[serde(default)]
    pub drift_sides: u32,
    /// Quick back-and-forths on the throttle and the steering while
    /// drifting (see `smoothness`).
    #[serde(default)]
    pub throttle_saws: u32,
    #[serde(default)]
    pub steer_saws: u32,
    /// Clips driven past forwards without paying (see
    /// `ScoreEventKind::ClipMissed`); `clips` counts the ones that paid.
    #[serde(default)]
    pub clips_missed: u32,
    /// Times the car drew level with a clip, drifting or not, with the sums
    /// of |drift angle| (degrees) and throttle (0..1) at those moments.
    #[serde(default)]
    pub clip_passes: u32,
    #[serde(default)]
    pub clip_angle_sum: f64,
    #[serde(default)]
    pub clip_throttle_sum: f64,
}

impl EpisodeStats {
    /// Average drift angle while drifting, degrees.
    pub fn average_angle(&self) -> f64 {
        if self.drift_time > 0.0 { self.angle_seconds / self.drift_time } else { 0.0 }
    }

    /// Share of the clips driven past that paid (None if it passed none).
    pub fn clip_rate(&self) -> Option<f64> {
        let passed = self.clips + self.clips_missed;
        (passed > 0).then(|| f64::from(self.clips) / f64::from(passed))
    }

    /// Average |drift angle| (degrees) and throttle when drawing level with
    /// a clip (None if it passed none).
    pub fn clip_angle_and_throttle(&self) -> Option<(f64, f64)> {
        let n = f64::from(self.clip_passes);
        (self.clip_passes > 0).then(|| (self.clip_angle_sum / n, self.clip_throttle_sum / n))
    }

    /// Average seconds each side of a drift was held.
    pub fn average_hold(&self) -> f64 {
        if self.drift_sides > 0 { self.drift_time / f64::from(self.drift_sides) } else { 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeResult {
    pub score: Breakdown,
    /// Best distance reached along the track, metres (laps keep adding).
    pub progress: f64,
    pub track_length: f64,
    pub time: f64,
    /// The run's time limit, seconds (0 when unknown, e.g. older files).
    #[serde(default)]
    pub max_secs: f64,
    pub end: EndReason,
    pub stats: EpisodeStats,
}

impl EpisodeResult {
    pub fn laps(&self) -> f64 {
        self.progress / self.track_length
    }
}

/// The rules that end a run, tracked tick by tick. Shared by training
/// episodes and the viewer's watch mode so both end runs the same way.
#[derive(Clone, Debug, Default)]
pub struct Termination {
    pub best_progress: f64,
    since_progress: f64,
    hard_hits: u32,
}

impl Termination {
    /// Feed in one tick: the car's progress and engine state, that tick's
    /// score events, and the time since the start. Returns why the run is
    /// over, if it is.
    pub fn update(
        &mut self,
        config: &EpisodeConfig,
        progress: f64,
        engine_running: bool,
        events: &[ScoreEvent],
        time: f64,
        dt: f64,
    ) -> Option<EndReason> {
        let spun = events.iter().any(|e| e.kind == ScoreEventKind::Spin);
        self.hard_hits += events.iter().filter(|e| e.kind == ScoreEventKind::HardHit).count() as u32;
        if progress > self.best_progress + 1.0 {
            self.best_progress = progress;
            self.since_progress = 0.0;
        } else {
            self.since_progress += dt;
        }

        if spun && config.end_on_spin {
            Some(EndReason::Spun)
        } else if !engine_running {
            Some(EndReason::Stalled)
        } else if self.hard_hits >= config.max_hard_hits {
            Some(EndReason::Crashed)
        } else if progress < self.best_progress - config.max_backwards {
            Some(EndReason::Backwards)
        } else if self.since_progress >= config.no_progress_secs {
            Some(EndReason::NoProgress)
        } else if time >= config.max_secs {
            Some(EndReason::TimeUp)
        } else {
            None
        }
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
    termination: Termination,
    stats: EpisodeStats,
    /// How long the clutch has been held in this press, seconds.
    clutch_held: f64,
    throttle_saw: SawDetector,
    steer_saw: SawDetector,
    end: Option<EndReason>,
    /// Recording for a replay, if asked for. Never changes the run.
    recorder: Option<Recorder>,
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
            termination: Termination::default(),
            stats: EpisodeStats::default(),
            clutch_held: 0.0,
            throttle_saw: SawDetector::default(),
            steer_saw: SawDetector::default(),
            end: None,
            recorder: None,
        }
    }

    /// Record this run for a replay (call before the first step).
    pub fn record(&mut self) {
        self.recorder = Some(Recorder::new(&self.world, &self.scorer));
    }

    /// The recording so far, ending it.
    pub fn take_recording(&mut self) -> Option<Recording> {
        self.recorder.take().map(|r| r.finish(&self.world, &self.scorer))
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
            if let Some(recorder) = &mut self.recorder {
                recorder.on_decision(&observation);
            }
            self.held = controller.decide(&observation);
        }
        self.advance(self.held)
    }

    /// Advance one tick with the given controls (no controller involved).
    pub fn advance(&mut self, controls: Controls) -> Option<EndReason> {
        self.world.step(controls, TICK);
        self.tick += 1;
        self.scorer.update(&self.world, self.world.contacts(), TICK);
        if let Some(recorder) = &mut self.recorder {
            recorder.on_tick(&self.world, &self.scorer);
        }
        let events = self.scorer.events();
        let stats = &mut self.stats;
        stats.ticks = self.tick;
        for e in events {
            match e.kind {
                ScoreEventKind::ZoneEnter => stats.zones_entered += 1,
                ScoreEventKind::Clip { .. } => stats.clips += 1,
                ScoreEventKind::ClipMissed => stats.clips_missed += 1,
                ScoreEventKind::Transition => stats.transitions += 1,
                ScoreEventKind::WallTap => stats.wall_taps += 1,
                ScoreEventKind::HardHit => stats.hard_hits += 1,
                ScoreEventKind::Spin => stats.spins += 1,
                _ => {}
            }
        }
        let passes = self.scorer.clip_passes;
        (stats.clip_passes, stats.clip_angle_sum, stats.clip_throttle_sum) = (passes.count, passes.angle_sum, passes.throttle_sum);
        let car = &self.world.car;
        let controls = car.state.controls;
        if controls.handbrake > car.params.brakes.handbrake_free_play {
            stats.handbrake_time += TICK;
        }
        let angle = if car.state.speed() > MOVING { car.state.body_slip_angle().abs().to_degrees() } else { 0.0 };
        if controls.clutch > car.params.clutch.free_play {
            stats.clutch_time += TICK;
            self.clutch_held += TICK;
            if self.clutch_held > self.config.clutch_grace_secs {
                stats.clutch_hold_time += TICK;
                stats.clutch_hold_angle += (angle / CLUTCH_FULL_ANGLE_DEG).min(1.0) * TICK;
            }
        } else {
            self.clutch_held = 0.0;
        }
        stats.max_angle = stats.max_angle.max(angle);
        if angle > 20.0 {
            stats.above_20_time += TICK;
        }
        if crate::car::engine::in_bite_zone(&car.params.clutch, controls.clutch) {
            stats.clutch_pedal_time += TICK;
            if !car.state.drivetrain.clutch_locked && car.state.engine_running() {
                stats.clutch_slip_time += TICK;
            }
        }
        stats.peak_clutch_temp = stats.peak_clutch_temp.max(self.world.car.state.drivetrain.clutch_temp);
        let time = self.tick as f64 * TICK;
        let smooth = &self.config.smoothness;
        let throttle_saw = self.throttle_saw.update(&smooth.throttle, controls.throttle, time);
        let steer_saw = self.steer_saw.update(&smooth.steer, controls.steer, time);
        if self.scorer.drift.active {
            stats.throttle_saws += u32::from(throttle_saw);
            stats.steer_saws += u32::from(steer_saw);
            stats.drift_time += TICK;
            stats.angle_seconds += self.world.car.state.body_slip_angle().abs().to_degrees() * TICK;
            stats.flow_seconds += self.scorer.flow.credit * TICK;
        }
        stats.drift_sides = self.scorer.flow.sides;

        let running = self.world.car.state.engine_running();
        self.end = self.termination.update(&self.config, self.world.progress, running, events, time, TICK);
        if self.end == Some(EndReason::TimeUp) {
            // A drift still going at the bell counts.
            self.scorer.finish();
            if let Some(recorder) = &mut self.recorder {
                recorder.on_events(self.scorer.events());
            }
        }
        self.end
    }

    /// Drive until the run ends.
    pub fn run(mut self, controller: &mut dyn Controller) -> EpisodeResult {
        controller.reset();
        while self.step(controller).is_none() {}
        self.result()
    }

    /// Drive until the run ends, recording it for a replay.
    pub fn run_recorded(mut self, controller: &mut dyn Controller) -> (EpisodeResult, Recording) {
        self.record();
        controller.reset();
        while self.step(controller).is_none() {}
        let recording = self.take_recording().expect("recording");
        (self.result(), recording)
    }

    pub fn result(&self) -> EpisodeResult {
        EpisodeResult {
            score: self.scorer.total,
            progress: self.termination.best_progress.max(self.world.progress),
            track_length: self.world.track.length,
            time: self.tick as f64 * TICK,
            max_secs: self.config.max_secs,
            end: self.end.unwrap_or(EndReason::TimeUp),
            stats: self.stats,
        }
    }
}
