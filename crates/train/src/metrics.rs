//! Per-generation statistics: one CSV row and one console line each.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use evo::Island;
use evo::diversity::diversity;
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
            diversity: mean(islands.iter().map(|i| diversity(&i.genomes))),
            sigma: mean(islands.iter().flat_map(|i| i.genomes.iter().map(|g| g.sigma))),
            seconds,
            steps_per_sec: ticks as f64 / seconds.max(1e-9),
            test_fitness: None,
            test_laps: None,
        }
    }

    pub fn console_line(&self) -> String {
        let ends: Vec<String> = EndReason::ALL
            .iter()
            .zip(self.ends)
            .filter(|&(_, share)| share > 0.0)
            .map(|(e, share)| format!("{} {:.0}%", e.name(), share * 100.0))
            .collect();
        let test = match (self.test_fitness, self.test_laps) {
            (Some(f), Some(l)) => format!(" | test {f:.0} ({l:.2} laps)"),
            _ => String::new(),
        };
        format!(
            "gen {:>4} [{}] best {:>8.1} mean {:>8.1} | best car {:>5.0} m {:.2} laps, score {:.0} | {} | clutch pedal {:.0}% slip {:.0}% peak {:.0}°C | div {:.3} σ {:.3} | {:.1}s {:.1}M steps/s{test}",
            self.generation,
            self.stage,
            self.best,
            self.mean,
            self.best_progress,
            self.best_laps,
            self.best_score.total(),
            ends.join(" "),
            self.clutch_pedal * 100.0,
            self.clutch_slip * 100.0,
            self.clutch_peak,
            self.diversity,
            self.sigma,
            self.seconds,
            self.steps_per_sec / 1e6,
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
                ["diversity", "sigma", "seconds", "steps_per_sec", "test_fitness", "test_laps"].iter().map(|s| s.to_string()),
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
        for v in [r.diversity, r.sigma, r.seconds, r.steps_per_sec] {
            num(v);
        }
        let optional = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
        row.push(optional(r.test_fitness));
        row.push(optional(r.test_laps));
        writeln!(self.file, "{}", row.join(","))?;
        self.file.flush()
    }
}
