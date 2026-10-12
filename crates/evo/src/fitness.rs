//! Fitness, shaped by a curriculum: early stages reward simply getting round
//! the track; later stages bring in drift angle, then the line (zones and
//! clips), then style. Each stage can say when to move on.

use serde::{Deserialize, Serialize};
use sim::episode::EpisodeResult;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub name: String,
    /// Fitness per metre of progress round the track.
    pub progress: f64,
    /// Weights on the judged score's categories.
    pub line: f64,
    pub angle: f64,
    pub style: f64,
    /// Weight on penalty points (spins, crashes, bobbles).
    pub penalty: f64,
    /// When to move on to the next stage (none = final stage).
    #[serde(default)]
    pub advance: Option<Advance>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advance {
    pub metric: Metric,
    /// The generation's best genome must average at least this...
    pub at_least: f64,
    /// ...for this many generations in a row.
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
        })
    }
}

impl Stage {
    /// A genome's fitness: the average over its runs (one per track).
    pub fn fitness(&self, results: &[EpisodeResult]) -> f64 {
        mean(results, |r| {
            let s = &r.score;
            self.progress * r.progress + self.line * s.line + self.angle * s.angle + self.style * s.style
                - self.penalty * s.penalty
        })
    }
}

/// Which stage training is on, and progress towards the next.
#[derive(Clone, Debug)]
pub struct Curriculum {
    pub stages: Vec<Stage>,
    pub current: usize,
    streak: u32,
}

impl Curriculum {
    pub fn new(stages: Vec<Stage>, start: usize) -> Self {
        assert!(!stages.is_empty(), "the curriculum needs at least one stage");
        let current = start.min(stages.len() - 1);
        Self { stages, current, streak: 0 }
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
        if advance.metric.measure(best) >= advance.at_least {
            self.streak += 1;
        } else {
            self.streak = 0;
        }
        if self.streak >= advance.generations {
            self.current += 1;
            self.streak = 0;
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
            end: EndReason::TimeUp,
            stats: EpisodeStats::default(),
        }
    }

    fn stage(progress: f64, angle: f64, advance: Option<Advance>) -> Stage {
        Stage { name: "s".into(), progress, line: 0.0, angle, style: 0.0, penalty: 1.0, advance }
    }

    #[test]
    fn fitness_is_the_weighted_mean_over_runs() {
        let s = stage(1.0, 2.0, None);
        let f = s.fitness(&[result(100.0, 10.0, 0.0), result(300.0, 0.0, 50.0)]);
        assert_eq!(f, ((100.0 + 20.0) + (300.0 - 50.0)) / 2.0);
    }

    #[test]
    fn advances_after_a_streak() {
        let advance = Advance { metric: Metric::Progress, at_least: 500.0, generations: 2 };
        let mut c = Curriculum::new(vec![stage(1.0, 0.0, Some(advance)), stage(0.5, 1.0, None)], 0);
        assert!(!c.observe(&[result(600.0, 0.0, 0.0)]));
        assert!(!c.observe(&[result(400.0, 0.0, 0.0)]), "streak broken");
        assert!(!c.observe(&[result(600.0, 0.0, 0.0)]));
        assert!(c.observe(&[result(700.0, 0.0, 0.0)]));
        assert_eq!(c.current, 1);
        assert!(!c.observe(&[result(9999.0, 0.0, 0.0)]), "last stage stays");
    }
}
