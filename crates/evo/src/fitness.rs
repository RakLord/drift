//! Fitness, shaped by a curriculum.
//!
//! Drifting is rewarded from the very first generation, on flow (big angle,
//! each side held long, linked smoothly) rather than banked combo points, and
//! multiplies the driving reward so a car that slides and survives beats one
//! that grips: `fitness = (progress + time alive) x (1 + drift_bonus x
//! drift_quality) + judged score terms`.
//! Holding the clutch (rather than kicking it) scales a run's reward down,
//! harder the more sideways the car is; so does sawing the throttle or the
//! steering while drifting.
//! Shaping stages also soften the scoring rules (drift counts from a few
//! degrees, spins don't end the run); the final stage is judged for real.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use sim::episode::{EpisodeConfig, EpisodeResult};
use sim::scoring::ScoringParams;

use crate::{GaConfig, Objectives};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub name: String,
    /// Fitness per metre of progress round the track...
    pub progress: f64,
    /// ...credited for at most this many laps (none = unlimited).
    #[serde(default)]
    pub progress_cap_laps: Option<f64>,
    /// Fitness per second survived.
    #[serde(default)]
    pub time_alive: f64,
    /// Multiplies the progress + alive reward by (1 + drift_bonus x drift
    /// quality).
    #[serde(default)]
    pub drift_bonus: f64,
    /// Small fitness per second of handbrake or clutch use (0 = off).
    #[serde(default)]
    pub pedal_bonus: f64,
    /// Weights on the judged (banked) score's categories.
    pub line: f64,
    pub angle: f64,
    pub style: f64,
    /// Weight on penalty points (spins, crashes, bobbles).
    pub penalty: f64,
    /// Cost of holding the clutch (none = free).
    #[serde(default)]
    pub clutch: Option<ClutchPenalty>,
    /// Cost of sawing the throttle or steering while drifting (none = free).
    #[serde(default)]
    pub smoothness: Option<SmoothnessPenalty>,
    /// Softer rules while learning to drift (none = the real rules).
    #[serde(default)]
    pub shaping: Option<Shaping>,
    /// When to move on to the next stage (none = final stage).
    #[serde(default)]
    pub advance: Option<Advance>,
    /// Fine-tuning settings for this stage (none = the normal GA settings).
    #[serde(default)]
    pub polish: Option<Polish>,
}

/// Fine-tuning: smaller mutation steps, no random immigrants, and more
/// tracks per generation so a lucky draw counts for less. Entering a polish
/// stage resets every sigma to `sigma_start`, without reseeding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Polish {
    pub sigma_start: f64,
    #[serde(default)]
    pub sigma_max: Option<f64>,
    #[serde(default)]
    pub immigrants: Option<usize>,
    /// Generated tracks per generation (replaces `tracks.per_generation`).
    #[serde(default)]
    pub tracks_per_generation: Option<u32>,
}

/// Holding the clutch past a kick scales the run's reward by `1 - strength
/// x cost` (never below 0), where cost is the share of the run spent holding
/// it, each second weighted from `calm_weight` with no drift angle up to 1 at
/// a big angle (`sim::episode::CLUTCH_FULL_ANGLE_DEG`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClutchPenalty {
    pub strength: f64,
    pub calm_weight: f64,
}

impl ClutchPenalty {
    /// Share of the run spent holding the clutch, weighted by angle.
    pub fn cost(&self, r: &EpisodeResult) -> f64 {
        let s = &r.stats;
        let weighted = self.calm_weight * s.clutch_hold_time + (1.0 - self.calm_weight) * s.clutch_hold_angle;
        weighted / r.time.max(1e-9)
    }

    /// Share of the run's reward it keeps.
    pub fn factor(&self, r: &EpisodeResult) -> f64 {
        (1.0 - self.strength * self.cost(r)).max(0.0)
    }
}

/// Sawing the throttle or the steering while drifting (quick back-and-forths,
/// see `sim::smoothness`) scales the drifting rewards by `1 - throttle x
/// throttle saws per second - steer x steer saws per second` (never below
/// 0), per second spent drifting. Steering is cheaper: the car has no
/// self-steer, so catching a slide takes real steering.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmoothnessPenalty {
    pub throttle: f64,
    pub steer: f64,
}

/// Saw rates count from at least this much drifting, seconds, so a run
/// that barely drifted isn't judged on a blip.
const MIN_SAW_SECS: f64 = 1.0;

impl SmoothnessPenalty {
    /// Saws per second of drifting: (throttle, steer).
    pub fn rates(r: &EpisodeResult) -> (f64, f64) {
        let secs = r.stats.drift_time.max(MIN_SAW_SECS);
        (f64::from(r.stats.throttle_saws) / secs, f64::from(r.stats.steer_saws) / secs)
    }

    /// Share of the run's drifting reward it keeps.
    pub fn factor(&self, r: &EpisodeResult) -> f64 {
        let (throttle, steer) = Self::rates(r);
        (1.0 - self.throttle * throttle - self.steer * steer).max(0.0)
    }
}

/// Scoring and episode rules for shaping stages.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shaping {
    /// A drift counts (and earns angle points) from this many degrees.
    pub drift_start_angle_deg: f64,
    /// Speed multiplier on all drift points, flat at every speed.
    pub speed_multiplier: f64,
    pub end_on_spin: bool,
    pub bobble_penalty: f64,
}

impl Shaping {
    /// Apply to a copy of the real scoring and episode rules.
    pub fn apply(&self, scoring: &mut ScoringParams, episode: &mut EpisodeConfig) {
        let drift = &mut scoring.drift;
        drift.start_angle_deg = self.drift_start_angle_deg;
        drift.end_angle_deg = drift.end_angle_deg.min(self.drift_start_angle_deg * 0.6);
        scoring.speed.slow_multiplier = self.speed_multiplier;
        scoring.speed.fast_multiplier = self.speed_multiplier;
        scoring.style.bobble_penalty = self.bobble_penalty;
        // Credit from the shaping start angle, under any judged floor.
        scoring.angle.credit_from_deg = self.drift_start_angle_deg;
        episode.end_on_spin = self.end_on_spin;
    }
}

/// How much and how smoothly a run drifted, from flow (`sim::scoring::flow`):
/// 1.0 = a full-angle drift, each side held past the hold ramp, for the
/// whole time limit; up to 1 + the hold bonus for sides held longer still.
/// Quick flicks earn little. Doesn't depend on banking combos. A run that
/// ends early (a spin, a crash) loses the share of the time limit it didn't
/// drive, so spinning out late costs little and early costs a lot (runs
/// without a known limit are judged on the time they lasted).
pub fn drift_quality(r: &EpisodeResult) -> f64 {
    r.stats.flow_seconds / r.time.max(r.max_secs).max(1e-9)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advance {
    pub metric: Metric,
    /// The saved best genome's metric, averaged over its runs and then over
    /// the last `generations` generations of the stage, must reach this.
    /// A rolling mean rather than a streak: the per-generation value swings
    /// with the tracks drawn, so N passes in a row almost never happen.
    pub at_least: f64,
    /// The window, in generations. Nothing advances until it is full.
    pub generations: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Metres round the track.
    Progress,
    Angle,
    Line,
    Style,
    /// The judged score (line + angle + style - penalties).
    Score,
    /// See `drift_quality`.
    DriftQuality,
}

/// Mean of `f` over a genome's runs.
fn mean(results: &[EpisodeResult], f: impl Fn(&EpisodeResult) -> f64) -> f64 {
    if results.is_empty() { 0.0 } else { results.iter().map(f).sum::<f64>() / results.len() as f64 }
}

impl Metric {
    pub fn measure(self, results: &[EpisodeResult]) -> f64 {
        mean(results, |r| match self {
            Self::Progress => r.progress,
            Self::Angle => r.score.angle,
            Self::Line => r.score.line,
            Self::Style => r.score.style,
            Self::Score => r.score.total(),
            Self::DriftQuality => drift_quality(r),
        })
    }
}

impl Stage {
    /// The driving reward: progress (capped) plus time alive.
    fn base(&self, r: &EpisodeResult) -> f64 {
        let credited = self.progress_cap_laps.map_or(r.progress, |laps| r.progress.min(laps * r.track_length));
        self.progress * credited + self.time_alive * r.time
    }

    /// Share of a run's reward kept after the clutch penalty (1 = none).
    pub fn clutch_factor(&self, r: &EpisodeResult) -> f64 {
        self.clutch.as_ref().map_or(1.0, |c| c.factor(r))
    }

    /// Share of a run's drifting reward kept after the smoothness penalty
    /// (1 = none).
    pub fn smoothness_factor(&self, r: &EpisodeResult) -> f64 {
        self.smoothness.as_ref().map_or(1.0, |s| s.factor(r))
    }

    /// The judged score's categories, weighted (before any pedal penalty).
    fn judged_points(&self, r: &EpisodeResult) -> f64 {
        let s = &r.score;
        self.line * s.line + self.angle * s.angle + self.style * s.style
    }

    fn run_fitness(&self, r: &EpisodeResult) -> f64 {
        let base = self.base(r);
        let shaped = base * (1.0 + self.drift_bonus * drift_quality(r) * self.smoothness_factor(r));
        let pedals = self.pedal_bonus * (r.stats.handbrake_time + r.stats.clutch_time);
        let judged = self.judged_points(r) * self.smoothness_factor(r);
        (shaped + pedals + judged) * self.clutch_factor(r) - self.penalty * r.score.penalty
    }

    /// A genome's fitness: the average over its runs (one per track).
    pub fn fitness(&self, results: &[EpisodeResult]) -> f64 {
        mean(results, |r| self.run_fitness(r))
    }

    /// The three things selection trades off, each averaged over the runs:
    /// driving (progress / staying alive), drifting (drift quality) and the
    /// judged score (weighted categories less penalties; 0 while a stage
    /// gives them no weight). All are cut by the clutch penalty; the last
    /// two by the smoothness penalty too.
    pub fn objectives(&self, results: &[EpisodeResult]) -> Objectives {
        [
            mean(results, |r| self.base(r) * self.clutch_factor(r)),
            mean(results, |r| drift_quality(r) * self.clutch_factor(r) * self.smoothness_factor(r)),
            mean(results, |r| {
                self.judged_points(r) * self.clutch_factor(r) * self.smoothness_factor(r) - self.penalty * r.score.penalty
            }),
        ]
    }

    /// GA settings for this stage: the base ones, with any polish applied.
    pub fn ga(&self, base: &GaConfig) -> GaConfig {
        let mut ga = base.clone();
        if let Some(p) = &self.polish {
            ga.sigma_start = p.sigma_start;
            ga.sigma_max = p.sigma_max.unwrap_or(ga.sigma_max).max(p.sigma_start);
            ga.immigrants = p.immigrants.unwrap_or(ga.immigrants);
            // Polishing narrows in on what works: no burst of variety.
            ga.reseed_fraction = 0.0;
        }
        ga
    }

    /// Generated tracks per generation for this stage.
    pub fn tracks_per_generation(&self, base: u32) -> u32 {
        self.polish.as_ref().and_then(|p| p.tracks_per_generation).unwrap_or(base)
    }

    /// Scoring and episode rules for this stage, from the real ones.
    pub fn rules(&self, scoring: &ScoringParams, episode: &EpisodeConfig) -> (ScoringParams, EpisodeConfig) {
        let (mut scoring, mut episode) = (scoring.clone(), episode.clone());
        if let Some(shaping) = &self.shaping {
            shaping.apply(&mut scoring, &mut episode);
        }
        (scoring, episode)
    }
}

/// Which stage training is on, and progress towards the next.
#[derive(Clone, Debug)]
pub struct Curriculum {
    pub stages: Vec<Stage>,
    pub current: usize,
    /// The advance metric for this stage's recent generations, newest last.
    /// Starts empty on --resume.
    recent: VecDeque<f64>,
}

impl Curriculum {
    pub fn new(stages: Vec<Stage>, start: usize) -> Self {
        assert!(!stages.is_empty(), "the curriculum needs at least one stage");
        let current = start.min(stages.len() - 1);
        Self { stages, current, recent: VecDeque::new() }
    }

    pub fn stage(&self) -> &Stage {
        &self.stages[self.current]
    }

    /// Feed in the generation's best genome's runs. Returns true when this
    /// moves training on to the next stage.
    pub fn observe(&mut self, best: &[EpisodeResult]) -> bool {
        let Some(advance) = self.stage().advance.clone() else { return false };
        if self.current + 1 >= self.stages.len() {
            return false;
        }
        let window = advance.generations.max(1) as usize;
        self.recent.push_back(advance.metric.measure(best));
        while self.recent.len() > window {
            self.recent.pop_front();
        }
        let average = self.recent.iter().sum::<f64>() / self.recent.len() as f64;
        if self.recent.len() == window && average >= advance.at_least {
            self.current += 1;
            self.recent.clear();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::episode::{EndReason, EpisodeStats};
    use sim::scoring::Breakdown;

    fn result(progress: f64, angle: f64, penalty: f64) -> EpisodeResult {
        EpisodeResult {
            score: Breakdown { line: 0.0, angle, style: 0.0, penalty },
            progress,
            track_length: 1000.0,
            time: 30.0,
            max_secs: 0.0,
            end: EndReason::TimeUp,
            stats: EpisodeStats::default(),
        }
    }

    fn stage(progress: f64, angle: f64, advance: Option<Advance>) -> Stage {
        Stage {
            name: "s".into(),
            progress,
            progress_cap_laps: None,
            time_alive: 0.0,
            drift_bonus: 0.0,
            pedal_bonus: 0.0,
            line: 0.0,
            angle,
            style: 0.0,
            penalty: 1.0,
            clutch: None,
            smoothness: None,
            shaping: None,
            advance,
            polish: None,
        }
    }

    #[test]
    fn drifting_multiplies_the_driving_reward() {
        let s = Stage { drift_bonus: 2.0, penalty: 0.0, ..stage(1.0, 0.0, None) };
        let grip = result(500.0, 0.0, 0.0);
        // Same distance, but 10 s of full flow credit over a 30 s run.
        let mut slide = result(500.0, 0.0, 0.0);
        slide.stats.flow_seconds = 10.0;
        let quality = drift_quality(&slide);
        assert!((quality - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.fitness(&[grip]), 500.0);
        assert!((s.fitness(&[slide]) - 500.0 * (1.0 + 2.0 * quality)).abs() < 1e-9);
        assert_eq!(s.objectives(&[slide])[0], 500.0);
    }

    #[test]
    fn holding_the_clutch_costs_more_at_angle() {
        let s = Stage { penalty: 0.0, clutch: Some(ClutchPenalty { strength: 2.0, calm_weight: 0.25 }), ..stage(1.0, 0.0, None) };
        let mut kicks = result(600.0, 0.0, 0.0);
        kicks.stats.clutch_time = 3.0;
        // 9 s of a 30 s run held in: straight, then the same at full angle.
        let mut straight = kicks;
        straight.stats.clutch_hold_time = 9.0;
        let mut sideways = straight;
        sideways.stats.clutch_hold_angle = 9.0;
        assert_eq!(s.fitness(&[kicks]), 600.0);
        assert!((s.fitness(&[straight]) - 600.0 * (1.0 - 2.0 * 0.25 * 0.3)).abs() < 1e-9);
        assert!((s.fitness(&[sideways]) - 600.0 * (1.0 - 2.0 * 0.3)).abs() < 1e-9);
        assert!((s.objectives(&[sideways])[0] - 600.0 * 0.4).abs() < 1e-9);
        // Never flips the reward negative.
        sideways.stats.clutch_hold_angle = 30.0;
        sideways.stats.clutch_hold_time = 30.0;
        assert_eq!(s.fitness(&[sideways]), 0.0);
    }

    #[test]
    fn ending_early_costs_the_flow_it_did_not_drive() {
        // Full flow credit every second until the run ends.
        let ended_at = |secs: f64| {
            let mut r = result(500.0, 0.0, 0.0);
            (r.time, r.max_secs, r.stats.flow_seconds) = (secs, 60.0, secs);
            drift_quality(&r)
        };
        assert!((ended_at(60.0) - 1.0).abs() < 1e-9);
        assert!((ended_at(55.0) - 55.0 / 60.0).abs() < 1e-9, "a spin on the last corner costs ~8%");
        assert!((ended_at(10.0) - 10.0 / 60.0).abs() < 1e-9, "a spin at 10 s costs ~83%");
        // No known limit (older files): judged on the time it lasted.
        let mut old = result(500.0, 0.0, 0.0);
        old.stats.flow_seconds = 15.0;
        assert!((drift_quality(&old) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn sawing_cuts_the_drifting_rewards_not_the_driving() {
        let smoothness = SmoothnessPenalty { throttle: 0.2, steer: 0.05 };
        let s = Stage { drift_bonus: 1.0, penalty: 0.0, smoothness: Some(smoothness), ..stage(1.0, 1.0, None) };
        let mut smooth = result(600.0, 400.0, 0.0);
        smooth.stats.drift_time = 20.0;
        smooth.stats.flow_seconds = 15.0;
        // 2 throttle and 4 steer saws per second drifting: 1 - 0.4 - 0.2.
        let mut sawing = smooth;
        sawing.stats.throttle_saws = 40;
        sawing.stats.steer_saws = 80;
        assert!((s.smoothness_factor(&sawing) - 0.4).abs() < 1e-9);
        assert_eq!(s.smoothness_factor(&smooth), 1.0);
        let (a, b) = (s.objectives(&[smooth]), s.objectives(&[sawing]));
        assert_eq!(a[0], b[0], "driving isn't judged on smoothness");
        assert!((b[1] - 0.4 * a[1]).abs() < 1e-9 && (b[2] - 0.4 * a[2]).abs() < 1e-9);
        assert!(s.fitness(&[sawing]) < s.fitness(&[smooth]));
        // A blip of drifting isn't judged as a rate over a blip.
        let mut blip = smooth;
        blip.stats.drift_time = 0.1;
        blip.stats.throttle_saws = 1;
        assert!((s.smoothness_factor(&blip) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn judged_objective_weighs_categories_and_penalties() {
        let s = Stage { penalty: 0.5, ..stage(0.1, 1.0, None) };
        let o = s.objectives(&[result(500.0, 300.0, 200.0)]);
        assert_eq!(o[2], 300.0 - 0.5 * 200.0);
        let no_weights = stage(1.0, 0.0, None);
        assert_eq!(no_weights.objectives(&[result(500.0, 300.0, 0.0)])[2], 0.0, "no judged weights, nothing to rank on");
    }

    #[test]
    fn progress_cap_and_time_alive() {
        let s = Stage { progress_cap_laps: Some(1.0), time_alive: 5.0, penalty: 0.0, ..stage(1.0, 0.0, None) };
        // 1500 m on a 1000 m track in 30 s: credited 1000 m + 150 for time.
        assert_eq!(s.fitness(&[result(1500.0, 0.0, 0.0)]), 1150.0);
    }

    #[test]
    fn shaping_softens_the_rules() {
        let shaping = Shaping { drift_start_angle_deg: 5.0, speed_multiplier: 1.0, end_on_spin: false, bobble_penalty: 0.0 };
        let s = Stage { shaping: Some(shaping), ..stage(1.0, 0.0, None) };
        let real = ScoringParams::default();
        let episode = EpisodeConfig {
            max_secs: 60.0,
            start_speed_kmh: 50.0,
            decision_every: 4,
            sensor_noise: 0.0,
            no_progress_secs: 4.0,
            max_backwards: 20.0,
            max_hard_hits: 3,
            end_on_spin: true,
            clutch_grace_secs: 0.4,
            smoothness: Default::default(),
            steering: Default::default(),
        };
        let (scoring, episode) = s.rules(&real, &episode);
        assert_eq!(scoring.drift.start_angle_deg, 5.0);
        assert!(scoring.drift.end_angle_deg < 5.0);
        assert_eq!(scoring.speed.multiplier(20.0), 1.0);
        assert_eq!(scoring.style.bobble_penalty, 0.0);
        assert!(!episode.end_on_spin);
        // A stage without shaping keeps the real rules.
        let (unchanged, _) = stage(1.0, 0.0, None).rules(&real, &episode);
        assert_eq!(unchanged.drift.start_angle_deg, real.drift.start_angle_deg);
    }

    #[test]
    fn polish_narrows_the_ga_settings() {
        let base = GaConfig {
            population: 20,
            elite_fraction: 0.1,
            tournament_size: 3,
            crossover_rate: 0.7,
            mutation_rate: 0.1,
            sigma_start: 0.1,
            sigma_min: 0.005,
            sigma_max: 0.5,
            sigma_adapt: 0.2,
            reset_rate: 0.001,
            immigrants: 2,
            migration_every: 10,
            migration_gap: 0.8,
            selection: crate::Selection::Pareto,
            reseed_fraction: 0.3,
            reseed_sigma: 0.3,
        };
        let plain = stage(1.0, 0.0, None);
        assert_eq!(plain.ga(&base).sigma_start, 0.1);
        assert_eq!(plain.tracks_per_generation(3), 3);

        let polish = Polish { sigma_start: 0.02, sigma_max: Some(0.1), immigrants: Some(0), tracks_per_generation: Some(8) };
        let s = Stage { polish: Some(polish), ..stage(1.0, 0.0, None) };
        let ga = s.ga(&base);
        assert_eq!((ga.sigma_start, ga.sigma_max, ga.immigrants, ga.reseed_fraction), (0.02, 0.1, 0, 0.0));
        assert_eq!(ga.population, base.population, "everything else unchanged");
        assert_eq!(s.tracks_per_generation(3), 8);
        // Only sigma_start given: the rest stay as they were.
        let s = Stage { polish: Some(Polish { sigma_start: 0.02, sigma_max: None, immigrants: None, tracks_per_generation: None }), ..s };
        assert_eq!((s.ga(&base).sigma_max, s.ga(&base).immigrants, s.tracks_per_generation(3)), (0.5, 2, 3));
    }

    #[test]
    fn fitness_is_the_weighted_mean_over_runs() {
        let s = stage(1.0, 2.0, None);
        let f = s.fitness(&[result(100.0, 10.0, 0.0), result(300.0, 0.0, 50.0)]);
        assert_eq!(f, ((100.0 + 20.0) + (300.0 - 50.0)) / 2.0);
    }

    #[test]
    fn advances_on_a_rolling_mean() {
        let advance = Advance { metric: Metric::Progress, at_least: 500.0, generations: 3 };
        let mut c = Curriculum::new(vec![stage(1.0, 0.0, Some(advance)), stage(0.5, 1.0, None)], 0);
        assert!(!c.observe(&[result(900.0, 0.0, 0.0)]), "window not full yet");
        assert!(!c.observe(&[result(900.0, 0.0, 0.0)]), "window not full yet");
        // Mean of 900, 900, 100 is 633: one bad generation doesn't block it.
        assert!(c.observe(&[result(100.0, 0.0, 0.0)]));
        assert_eq!(c.current, 1);
        assert!(!c.observe(&[result(9999.0, 0.0, 0.0)]), "last stage stays");
    }

    #[test]
    fn a_low_mean_holds_and_old_generations_drop_out() {
        let advance = Advance { metric: Metric::Progress, at_least: 500.0, generations: 2 };
        let mut c = Curriculum::new(vec![stage(1.0, 0.0, Some(advance)), stage(0.5, 1.0, None)], 0);
        assert!(!c.observe(&[result(100.0, 0.0, 0.0)]));
        assert!(!c.observe(&[result(600.0, 0.0, 0.0)]), "mean 350");
        assert!(c.observe(&[result(600.0, 0.0, 0.0)]), "the 100 has dropped out");
    }
}
