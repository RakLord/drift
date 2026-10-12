//! Per-generation statistics: one CSV row and one console line each.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use evo::Island;
use evo::diversity::{behaviour, behaviour_diversity, diversity};
use evo::fitness::drift_quality;
use sim::episode::{EndReason, EpisodeResult};
use sim::scoring::Breakdown;

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub generation: u32,
    pub stage: String,
    pub best: f64,
    pub mean: f64,
    pub median: f64,
    pub worst: f64,
    pub island_best: Vec<f64>,
    /// The best genome's mean over its runs.
    pub best_score: Breakdown,
    pub best_progress: f64,
    pub best_laps: f64,
    /// Population means over every run.
    pub mean_score: Breakdown,
    pub mean_progress: f64,
    pub drift_share: f64,
    pub drift_angle: f64,
    pub zones: f64,
    /// Share of driving time with the clutch pedal in the bite zone, share
    /// actually slipping there (riding it), and the mean hottest clutch.
    pub clutch_pedal: f64,
    pub clutch_slip: f64,
    pub clutch_peak: f64,
    /// Share of runs ending each way, in `EndReason::ALL` order.
    pub ends: [f64; 6],
    /// Drift quality (raw angle-time, see `evo::fitness::drift_quality`):
    /// the best genome's mean, and the population's.
    pub best_quality: f64,
    pub mean_quality: f64,
    /// The top drifter: the genome with the highest drift quality (often
    /// not the fittest). Its drift quality, average drift angle (deg),
    /// judged score and laps.
    pub top_quality: f64,
    pub top_angle: f64,
    pub top_score: f64,
    pub top_laps: f64,
    /// Spins per run, across the population.
    pub spins: f64,
    /// Share of time with the handbrake on / clutch past its free play.
    pub handbrake_share: f64,
    pub clutch_share: f64,
    /// Largest drift angle per run: median and 90th percentile, degrees.
    pub max_angle_p50: f64,
    pub max_angle_p90: f64,
    /// Share of time above 20 degrees.
    pub above_20_share: f64,
    /// Spread of the population in behaviour space (0 = all drive alike).
    pub behaviour_diversity: f64,
    /// Spread in weight space.
    pub diversity: f64,
    pub sigma: f64,
    pub seconds: f64,
    pub steps_per_sec: f64,
    /// Best genome on the library tracks, when tested this generation.
    pub test_fitness: Option<f64>,
    pub test_laps: Option<f64>,
}

fn mean_score(results: &[&EpisodeResult]) -> Breakdown {
    let n = results.len().max(1) as f64;
    let sum = results.iter().fold(Breakdown::default(), |acc, r| acc + r.score);
    Breakdown { line: sum.line / n, angle: sum.angle / n, style: sum.style / n, penalty: sum.penalty / n }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0), |(s, c), v| (s + v, c + 1));
    if count == 0 { 0.0 } else { sum / f64::from(count) }
}

impl Report {
    /// Summarise an evaluated generation. `best` is (island, genome) of the
    /// fittest genome.
    pub fn new(
        generation: u32,
        stage: &str,
        islands: &[Island],
        results: &[Vec<Vec<EpisodeResult>>],
        best: (usize, usize),
        seconds: f64,
    ) -> Self {
        let mut fitness: Vec<f64> = islands.iter().flat_map(|i| i.fitness.iter().copied()).collect();
        fitness.sort_by(f64::total_cmp);
        let all: Vec<&EpisodeResult> = results.iter().flatten().flatten().collect();
        let best_runs: Vec<&EpisodeResult> = results[best.0][best.1].iter().collect();
        let ticks: u64 = all.iter().map(|r| r.stats.ticks).sum();
        let drifting: Vec<&&EpisodeResult> = all.iter().filter(|r| r.stats.drift_time > 0.0).collect();

        let share = |f: fn(&EpisodeResult) -> f64| mean(all.iter().map(|r| f(r) / r.time.max(1e-9)));
        let mut max_angles: Vec<f64> = all.iter().map(|r| r.stats.max_angle).collect();
        max_angles.sort_by(f64::total_cmp);
        let percentile = |p: f64| max_angles.get(((max_angles.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
        let behaviours: Vec<[f64; 5]> = results.iter().flatten().map(|runs| behaviour(runs)).collect();
        let quality_of = |runs: &[EpisodeResult]| mean(runs.iter().map(drift_quality));
        let top_runs: &[EpisodeResult] = results
            .iter()
            .flatten()
            .max_by(|a, b| quality_of(a).total_cmp(&quality_of(b)))
            .map_or(&[], Vec::as_slice);
        let drifting_angle = |runs: &[EpisodeResult]| {
            let time: f64 = runs.iter().map(|r| r.stats.drift_time).sum();
            let angle: f64 = runs.iter().map(|r| r.stats.angle_seconds).sum();
            if time > 0.0 { angle / time } else { 0.0 }
        };
        let mut ends = [0.0; 6];
        for r in &all {
            let k = EndReason::ALL.iter().position(|&e| e == r.end).unwrap_or(0);
            ends[k] += 1.0 / all.len().max(1) as f64;
        }
        Self {
            generation,
            stage: stage.to_owned(),
            best: *fitness.last().unwrap_or(&0.0),
            mean: mean(fitness.iter().copied()),
            median: fitness.get(fitness.len() / 2).copied().unwrap_or(0.0),
            worst: *fitness.first().unwrap_or(&0.0),
            island_best: islands.iter().map(|i| i.best().map_or(0.0, |(_, f)| f)).collect(),
            best_score: mean_score(&best_runs),
            best_progress: mean(best_runs.iter().map(|r| r.progress)),
            best_laps: mean(best_runs.iter().map(|r| r.laps())),
            mean_score: mean_score(&all),
            mean_progress: mean(all.iter().map(|r| r.progress)),
            drift_share: mean(all.iter().map(|r| r.stats.drift_time / r.time.max(1e-9))),
            drift_angle: mean(drifting.iter().map(|r| r.stats.average_angle())),
            zones: mean(all.iter().map(|r| f64::from(r.stats.zones_entered + r.stats.clips))),
            clutch_pedal: mean(all.iter().map(|r| r.stats.clutch_pedal_time / r.time.max(1e-9))),
            clutch_slip: mean(all.iter().map(|r| r.stats.clutch_slip_time / r.time.max(1e-9))),
            clutch_peak: mean(all.iter().map(|r| r.stats.peak_clutch_temp)),
            ends,
            best_quality: mean(best_runs.iter().map(|r| drift_quality(r))),
            top_quality: quality_of(top_runs),
            top_angle: drifting_angle(top_runs),
            top_score: mean(top_runs.iter().map(|r| r.score.total())),
            top_laps: mean(top_runs.iter().map(|r| r.laps())),
            spins: mean(all.iter().map(|r| f64::from(r.stats.spins))),
            mean_quality: mean(all.iter().map(|r| drift_quality(r))),
            handbrake_share: share(|r| r.stats.handbrake_time),
            clutch_share: share(|r| r.stats.clutch_time),
            max_angle_p50: percentile(0.5),
            max_angle_p90: percentile(0.9),
            above_20_share: share(|r| r.stats.above_20_time),
            behaviour_diversity: behaviour_diversity(&behaviours),
            diversity: mean(islands.iter().map(|i| diversity(&i.genomes))),
            sigma: mean(islands.iter().flat_map(|i| i.genomes.iter().map(|g| g.sigma))),
            seconds,
            steps_per_sec: ticks as f64 / seconds.max(1e-9),
            test_fitness: None,
            test_laps: None,
        }
    }

    /// What the console columns mean; printed once at the start.
    pub fn legend() -> &'static str {
        "drift q = angle-time (1.0 = a 45° drift held the whole run). top = the most drift-y genome, \
         best = the fittest. score = banked judged points. angle = typical biggest angle per run. \
         ends: T time up, NP no progress, CR crashed, ST stalled, SP spun, BK backwards"
    }

    /// Column headings; printed at the start and every so often.
    pub fn header() -> String {
        [
            "  gen stage │ ── top drifter ──────────── │ ── fittest ────────── │ ── population ────────────────── │ ends %            │ pedals %        │",
            "            │ drift q  angle  score  laps │ fitness drift q  laps │ drift q  >20°  angle spins  bdiv │  T NP CR ST SP BK │  hb clutch slip │ secs",
        ]
        .join("\n")
    }

    pub fn console_line(&self) -> String {
        let end = |reason: EndReason| {
            let k = EndReason::ALL.iter().position(|&e| e == reason).unwrap_or(0);
            format!("{:>2.0}", self.ends[k] * 100.0)
        };
        let test = match (self.test_fitness, self.test_laps) {
            (Some(f), Some(l)) => format!("  test: fitness {f:.0}, {l:.2} laps"),
            _ => String::new(),
        };
        format!(
            "{:>5} {:<6}│   {:>5.2} {:>5.0}° {:>6.0} {:>5.2} │ {:>7.0}   {:>5.2} {:>5.2} │   {:>5.2} {:>4.0}% {:>5.0}° {:>5.1} {:>5.3} │ {} {} {} {} {} {} │ {:>3.0} {:>6.0} {:>4.0} │ {:>4.1}{test}",
            self.generation,
            self.stage,
            self.top_quality,
            self.top_angle,
            self.top_score,
            self.top_laps,
            self.best,
            self.best_quality,
            self.best_laps,
            self.mean_quality,
            self.above_20_share * 100.0,
            self.max_angle_p50,
            self.spins,
            self.behaviour_diversity,
            end(EndReason::TimeUp),
            end(EndReason::NoProgress),
            end(EndReason::Crashed),
            end(EndReason::Stalled),
            end(EndReason::Spun),
            end(EndReason::Backwards),
            self.handbrake_share * 100.0,
            self.clutch_share * 100.0,
            self.clutch_slip * 100.0,
            self.seconds,
        )
    }
}

/// Appends rows to `metrics.csv`, writing the header for a new file.
pub struct MetricsCsv {
    file: File,
}

impl MetricsCsv {
    pub fn open(path: &Path, islands: usize) -> io::Result<Self> {
        let new = !path.exists();
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        if new {
            let mut columns: Vec<String> = ["generation", "stage", "best", "mean", "median", "worst"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            columns.extend((0..islands).map(|i| format!("island{i}_best")));
            columns.extend(
                [
                    "best_line",
                    "best_angle",
                    "best_style",
                    "best_penalty",
                    "best_progress",
                    "best_laps",
                    "mean_line",
                    "mean_angle",
                    "mean_style",
                    "mean_penalty",
                    "mean_progress",
                    "drift_share",
                    "drift_angle",
                    "zones",
                    "clutch_pedal",
                    "clutch_slip",
                    "clutch_peak",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            columns.extend(EndReason::ALL.iter().map(|e| format!("end_{}", e.name().replace(' ', "_"))));
            columns.extend(
                [
                    "best_quality",
                    "mean_quality",
                    "top_quality",
                    "top_angle",
                    "top_score",
                    "top_laps",
                    "spins",
                    "handbrake_share",
                    "clutch_share",
                    "max_angle_p50",
                    "max_angle_p90",
                    "above_20_share",
                    "behaviour_diversity",
                    "diversity",
                    "sigma",
                    "seconds",
                    "steps_per_sec",
                    "test_fitness",
                    "test_laps",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            writeln!(file, "{}", columns.join(","))?;
        }
        Ok(Self { file })
    }

    pub fn write(&mut self, r: &Report) -> io::Result<()> {
        let mut row = vec![r.generation.to_string(), r.stage.clone()];
        let mut num = |v: f64| row.push(format!("{v:.4}"));
        for v in [r.best, r.mean, r.median, r.worst] {
            num(v);
        }
        for &v in &r.island_best {
            num(v);
        }
        let (b, m) = (r.best_score, r.mean_score);
        for v in [b.line, b.angle, b.style, b.penalty, r.best_progress, r.best_laps] {
            num(v);
        }
        for v in [
            m.line,
            m.angle,
            m.style,
            m.penalty,
            r.mean_progress,
            r.drift_share,
            r.drift_angle,
            r.zones,
            r.clutch_pedal,
            r.clutch_slip,
            r.clutch_peak,
        ] {
            num(v);
        }
        for v in r.ends {
            num(v);
        }
        for v in [
            r.best_quality,
            r.mean_quality,
            r.top_quality,
            r.top_angle,
            r.top_score,
            r.top_laps,
            r.spins,
            r.handbrake_share,
            r.clutch_share,
            r.max_angle_p50,
            r.max_angle_p90,
            r.above_20_share,
            r.behaviour_diversity,
            r.diversity,
            r.sigma,
            r.seconds,
            r.steps_per_sec,
        ] {
            num(v);
        }
        let optional = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
        row.push(optional(r.test_fitness));
        row.push(optional(r.test_laps));
        writeln!(self.file, "{}", row.join(","))?;
        self.file.flush()
    }
}
