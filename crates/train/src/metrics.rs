//! Per-generation statistics: one CSV row and one console line each.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use evo::Island;
use evo::diversity::{behaviour, behaviour_diversity, diversity};
use evo::fitness::{SmoothnessPenalty, drift_quality};
use sim::episode::{EndReason, EpisodeResult};
use sim::scoring::Breakdown;

/// The driver saved to best.genome this generation.
pub struct Saved {
    pub island: usize,
    pub genome: usize,
    /// Its runs this generation, plus any `[best]` check runs.
    pub runs: Vec<EpisodeResult>,
    /// The `[best]` check tracks' ids (see `tracks_log`), for the runs after
    /// the training ones.
    pub check_tracks: Vec<String>,
    /// Fitness over all of `runs`.
    pub fitness: f64,
    /// Where its fitness ranked this generation (0 = the fittest).
    pub rank: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub generation: u32,
    pub stage: String,
    pub best: f64,
    pub mean: f64,
    pub median: f64,
    pub worst: f64,
    pub island_best: Vec<f64>,
    /// The driver saved to best.genome: its fitness over all its runs
    /// (including any `[best]` check tracks), and where its fitness ranked
    /// this generation (1 = the fittest).
    pub saved_fitness: f64,
    pub saved_rank: usize,
    /// Replays written this generation.
    pub replays: usize,
    /// The saved driver's mean over all its runs.
    pub best_score: Breakdown,
    pub best_progress: f64,
    pub best_laps: f64,
    /// Population means over every run.
    pub mean_score: Breakdown,
    pub mean_progress: f64,
    pub drift_share: f64,
    pub drift_angle: f64,
    /// Mean seconds each side of a drift was held, over drifting runs.
    pub drift_hold: f64,
    pub zones: f64,
    /// Share of driving time with the clutch pedal in the bite zone, share
    /// actually slipping there (riding it), and the mean hottest clutch.
    pub clutch_pedal: f64,
    pub clutch_slip: f64,
    pub clutch_peak: f64,
    /// Share of runs ending each way, in `EndReason::ALL` order.
    pub ends: [f64; 6],
    /// Drift quality (flow, see `evo::fitness::drift_quality`):
    /// the best genome's mean, and the population's.
    pub best_quality: f64,
    pub mean_quality: f64,
    /// The top drifter: the genome with the highest drift quality (often
    /// not the fittest). Its drift quality, average drift angle (deg),
    /// seconds per drift side, judged score and laps.
    pub top_quality: f64,
    pub top_angle: f64,
    pub top_hold: f64,
    pub top_score: f64,
    pub top_laps: f64,
    /// Spins per run, across the population.
    pub spins: f64,
    /// Share of time with the handbrake on / clutch past its free play.
    pub handbrake_share: f64,
    pub clutch_share: f64,
    /// Share of time holding the clutch past a kick, and the same weighted
    /// by drift angle (what the clutch penalty charges for).
    pub clutch_hold: f64,
    pub clutch_hold_angle: f64,
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
    /// Saws per second of drifting (throttle, steer; see
    /// `evo::fitness::SmoothnessPenalty`): the saved driver's, and the
    /// population's.
    pub best_saws: (f64, f64),
    pub mean_saws: (f64, f64),
    /// The saved driver's judged objective (weighted judged score less
    /// penalties, after the pedal penalties) on its training runs.
    pub best_judged: f64,
    /// Share of the clips driven past that paid, over all of the saved
    /// driver's runs and over the population's.
    pub best_clip_rate: f64,
    pub mean_clip_rate: f64,
    /// The saved driver's average |drift angle| (deg) and throttle when
    /// drawing level with a clip, drifting or not.
    pub best_clip_angle: f64,
    pub best_clip_throttle: f64,
    /// Best genome on the library tracks, when tested this generation.
    pub test_fitness: Option<f64>,
    pub test_laps: Option<f64>,
}

pub(crate) fn mean_score(results: &[&EpisodeResult]) -> Breakdown {
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
        saved: &Saved,
        seconds: f64,
    ) -> Self {
        let mut fitness: Vec<f64> = islands.iter().flat_map(|i| i.fitness.iter().copied()).collect();
        fitness.sort_by(f64::total_cmp);
        let all: Vec<&EpisodeResult> = results.iter().flatten().flatten().collect();
        let best_runs: Vec<&EpisodeResult> = saved.runs.iter().collect();
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
        let drifting_hold = |runs: &[EpisodeResult]| {
            let time: f64 = runs.iter().map(|r| r.stats.drift_time).sum();
            let sides: u32 = runs.iter().map(|r| r.stats.drift_sides).sum();
            if sides > 0 { time / f64::from(sides) } else { 0.0 }
        };
        let saws = |runs: &[&EpisodeResult]| {
            let rates: Vec<(f64, f64)> = runs.iter().map(|r| SmoothnessPenalty::rates(r)).collect();
            (mean(rates.iter().map(|r| r.0)), mean(rates.iter().map(|r| r.1)))
        };
        let drifting_runs: Vec<&EpisodeResult> = drifting.iter().map(|r| **r).collect();
        // Pooled over the runs, so runs past no clips don't count.
        let clip_rate = |runs: &[&EpisodeResult]| {
            let hit: u32 = runs.iter().map(|r| r.stats.clips).sum();
            let missed: u32 = runs.iter().map(|r| r.stats.clips_missed).sum();
            if hit + missed > 0 { f64::from(hit) / f64::from(hit + missed) } else { 0.0 }
        };
        let best_passes: u32 = best_runs.iter().map(|r| r.stats.clip_passes).sum();
        let per_pass = |f: fn(&EpisodeResult) -> f64| {
            if best_passes > 0 { best_runs.iter().map(|r| f(r)).sum::<f64>() / f64::from(best_passes) } else { 0.0 }
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
            saved_fitness: saved.fitness,
            saved_rank: saved.rank + 1,
            replays: 0,
            best_score: mean_score(&best_runs),
            best_progress: mean(best_runs.iter().map(|r| r.progress)),
            best_laps: mean(best_runs.iter().map(|r| r.laps())),
            mean_score: mean_score(&all),
            mean_progress: mean(all.iter().map(|r| r.progress)),
            drift_share: mean(all.iter().map(|r| r.stats.drift_time / r.time.max(1e-9))),
            drift_angle: mean(drifting.iter().map(|r| r.stats.average_angle())),
            drift_hold: mean(drifting.iter().map(|r| r.stats.average_hold())),
            zones: mean(all.iter().map(|r| f64::from(r.stats.zones_entered + r.stats.clips))),
            clutch_pedal: mean(all.iter().map(|r| r.stats.clutch_pedal_time / r.time.max(1e-9))),
            clutch_slip: mean(all.iter().map(|r| r.stats.clutch_slip_time / r.time.max(1e-9))),
            clutch_peak: mean(all.iter().map(|r| r.stats.peak_clutch_temp)),
            ends,
            best_quality: mean(best_runs.iter().map(|r| drift_quality(r))),
            top_quality: quality_of(top_runs),
            top_angle: drifting_angle(top_runs),
            top_hold: drifting_hold(top_runs),
            top_score: mean(top_runs.iter().map(|r| r.score.total())),
            top_laps: mean(top_runs.iter().map(|r| r.laps())),
            spins: mean(all.iter().map(|r| f64::from(r.stats.spins))),
            mean_quality: mean(all.iter().map(|r| drift_quality(r))),
            handbrake_share: share(|r| r.stats.handbrake_time),
            clutch_share: share(|r| r.stats.clutch_time),
            clutch_hold: share(|r| r.stats.clutch_hold_time),
            clutch_hold_angle: share(|r| r.stats.clutch_hold_angle),
            max_angle_p50: percentile(0.5),
            max_angle_p90: percentile(0.9),
            above_20_share: share(|r| r.stats.above_20_time),
            behaviour_diversity: behaviour_diversity(&behaviours),
            diversity: mean(islands.iter().map(|i| diversity(&i.genomes))),
            sigma: mean(islands.iter().flat_map(|i| i.genomes.iter().map(|g| g.sigma))),
            seconds,
            steps_per_sec: ticks as f64 / seconds.max(1e-9),
            best_saws: saws(&best_runs),
            mean_saws: saws(&drifting_runs),
            best_judged: islands.get(saved.island).and_then(|i| i.objectives.get(saved.genome)).map_or(0.0, |o| o[2]),
            best_clip_rate: clip_rate(&best_runs),
            mean_clip_rate: clip_rate(&all),
            best_clip_angle: per_pass(|r| r.stats.clip_angle_sum),
            best_clip_throttle: per_pass(|r| r.stats.clip_throttle_sum),
            test_fitness: None,
            test_laps: None,
        }
    }

    /// What the console columns mean; printed once at the start.
    pub fn legend() -> &'static str {
        "drift q = flow (1.0 = a full-angle drift, each side held 1 s+, the whole run; flicks earn little). side = seconds per drift side. top = the most drift-y genome, \
         saved = the driver written to best.genome (fitness over all its runs; clip = share of the clips it drove past that paid; # = its fitness rank this generation). score = banked judged points. angle = typical biggest angle per run. \
         saws = the saved driver's quick throttle / steer back-and-forths per second drifting. \
         ends: T time up, NP no progress, CR crashed, ST stalled, SP spun, BK backwards"
    }

    /// Column headings; printed at the start and every so often.
    pub fn header() -> String {
        [
            "  gen stage │ ── top drifter ────────────────── │ ── saved best ────────────────── │ ── population ────────────────── │ ends %            │ pedals %             │ saws/s    │",
            "            │ drift q  angle  side  score  laps │ fitness drift q  laps  clip    # │ drift q  >20°  angle spins  bdiv │  T NP CR ST SP BK │  hb clutch slip hold │ thr steer │ secs",
        ]
        .join("\n")
    }

    pub fn console_line(&self) -> String {
        let end = |reason: EndReason| {
            let k = EndReason::ALL.iter().position(|&e| e == reason).unwrap_or(0);
            format!("{:>2.0}", self.ends[k] * 100.0)
        };
        let mut test = match (self.test_fitness, self.test_laps) {
            (Some(f), Some(l)) => format!("  test: fitness {f:.0}, {l:.2} laps"),
            _ => String::new(),
        };
        if self.replays > 0 {
            test += &format!("  [{} replays]", self.replays);
        }
        format!(
            "{:>5} {:<6}│   {:>5.2} {:>5.0}° {:>4.1}s {:>6.0} {:>5.2} │ {:>7.0}   {:>5.2} {:>5.2} {:>4.0}% {:>4} │   {:>5.2} {:>4.0}% {:>5.0}° {:>5.1} {:>5.3} │ {} {} {} {} {} {} │ {:>3.0} {:>6.0} {:>4.0} {:>4.0} │ {:>3.1} {:>5.1} │ {:>4.1}{test}",
            self.generation,
            self.stage,
            self.top_quality,
            self.top_angle,
            self.top_hold,
            self.top_score,
            self.top_laps,
            self.saved_fitness,
            self.best_quality,
            self.best_laps,
            self.best_clip_rate * 100.0,
            self.saved_rank,
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
            self.clutch_hold * 100.0,
            self.best_saws.0,
            self.best_saws.1,
            self.seconds,
        )
    }
}

/// Appends rows to `metrics.csv`, writing the header for a new file. Rows
/// follow the file's own header, so a resumed run keeps the columns it
/// started with (newer ones are left out, missing ones left blank).
pub struct MetricsCsv {
    file: File,
    columns: Vec<String>,
}

impl MetricsCsv {
    pub fn open(path: &Path, islands: usize) -> io::Result<Self> {
        let header = std::fs::read_to_string(path).ok().and_then(|text| text.lines().next().map(str::to_owned));
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        let columns = match header.filter(|h| !h.is_empty()) {
            Some(header) => header.split(',').map(str::to_owned).collect(),
            None => {
                let columns: Vec<String> = Report::default().fields(islands).into_iter().map(|(name, _)| name).collect();
                writeln!(file, "{}", columns.join(","))?;
                columns
            }
        };
        Ok(Self { file, columns })
    }

    pub fn write(&mut self, r: &Report) -> io::Result<()> {
        let fields = r.fields(r.island_best.len());
        let row: Vec<&str> = self
            .columns
            .iter()
            .map(|c| fields.iter().find(|(name, _)| name == c).map_or("", |(_, v)| v.as_str()))
            .collect();
        writeln!(self.file, "{}", row.join(","))?;
        self.file.flush()
    }
}

impl Report {
    /// Every CSV column, in order, with this report's value.
    fn fields(&self, islands: usize) -> Vec<(String, String)> {
        let mut out = vec![("generation".to_owned(), self.generation.to_string()), ("stage".to_owned(), self.stage.clone())];
        let mut num = |name: &str, v: f64| out.push((name.to_owned(), format!("{v:.4}")));
        for (name, v) in [("best", self.best), ("mean", self.mean), ("median", self.median), ("worst", self.worst)] {
            num(name, v);
        }
        for i in 0..islands {
            num(&format!("island{i}_best"), self.island_best.get(i).copied().unwrap_or(0.0));
        }
        let (b, m) = (self.best_score, self.mean_score);
        for (name, v) in [
            ("best_line", b.line),
            ("best_angle", b.angle),
            ("best_style", b.style),
            ("best_penalty", b.penalty),
            ("best_progress", self.best_progress),
            ("best_laps", self.best_laps),
            ("mean_line", m.line),
            ("mean_angle", m.angle),
            ("mean_style", m.style),
            ("mean_penalty", m.penalty),
            ("mean_progress", self.mean_progress),
            ("drift_share", self.drift_share),
            ("drift_angle", self.drift_angle),
            ("drift_hold", self.drift_hold),
            ("zones", self.zones),
            ("clutch_pedal", self.clutch_pedal),
            ("clutch_slip", self.clutch_slip),
            ("clutch_peak", self.clutch_peak),
        ] {
            num(name, v);
        }
        for (reason, v) in EndReason::ALL.iter().zip(self.ends) {
            num(&format!("end_{}", reason.name().replace(' ', "_")), v);
        }
        for (name, v) in [
            ("best_quality", self.best_quality),
            ("mean_quality", self.mean_quality),
            ("top_quality", self.top_quality),
            ("top_angle", self.top_angle),
            ("top_hold", self.top_hold),
            ("top_score", self.top_score),
            ("top_laps", self.top_laps),
            ("spins", self.spins),
            ("handbrake_share", self.handbrake_share),
            ("clutch_share", self.clutch_share),
            ("clutch_hold", self.clutch_hold),
            ("clutch_hold_angle", self.clutch_hold_angle),
            ("max_angle_p50", self.max_angle_p50),
            ("max_angle_p90", self.max_angle_p90),
            ("above_20_share", self.above_20_share),
            ("behaviour_diversity", self.behaviour_diversity),
            ("diversity", self.diversity),
            ("sigma", self.sigma),
            ("seconds", self.seconds),
            ("steps_per_sec", self.steps_per_sec),
            ("best_throttle_saws", self.best_saws.0),
            ("best_steer_saws", self.best_saws.1),
            ("mean_throttle_saws", self.mean_saws.0),
            ("mean_steer_saws", self.mean_saws.1),
            ("best_judged", self.best_judged),
            ("best_clip_rate", self.best_clip_rate),
            ("mean_clip_rate", self.mean_clip_rate),
            ("best_clip_angle", self.best_clip_angle),
            ("best_clip_throttle", self.best_clip_throttle),
        ] {
            num(name, v);
        }
        let optional = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.4}"));
        out.push(("test_fitness".to_owned(), optional(self.test_fitness)));
        out.push(("test_laps".to_owned(), optional(self.test_laps)));
        out.push(("saved_fitness".to_owned(), format!("{:.4}", self.saved_fitness)));
        out.push(("saved_rank".to_owned(), self.saved_rank.to_string()));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_files_own_header() {
        let dir = std::env::temp_dir().join(format!("drift-metrics-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report = Report { generation: 7, stage: "slide".into(), best: 12.5, saved_rank: 2, island_best: vec![1.0], ..Report::default() };

        // A new file gets every column.
        let fresh = dir.join("fresh.csv");
        MetricsCsv::open(&fresh, 1).unwrap().write(&report).unwrap();
        let text = std::fs::read_to_string(&fresh).unwrap();
        let mut lines = text.lines();
        let (header, row) = (lines.next().unwrap(), lines.next().unwrap());
        assert_eq!(header.split(',').count(), row.split(',').count());
        assert!(header.ends_with("saved_fitness,saved_rank") && row.ends_with(",2"));

        // An older file (resumed run) keeps its columns, in its order.
        let old = dir.join("old.csv");
        std::fs::write(&old, "stage,generation,best,gone\n").unwrap();
        MetricsCsv::open(&old, 1).unwrap().write(&report).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "stage,generation,best,gone\nslide,7,12.5000,\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
