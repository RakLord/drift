//! A run's per-track results over its last N generations, from
//! `runs/<run>/tracks.csv`: each library track's runs averaged (worst
//! first), and the generated tracks it did worst on. Generated tracks are
//! driven once each, so they're ranked run by run. Polled, so it follows a
//! live run.

use std::collections::HashMap;
use std::sync::Arc;

use sim::track::generator::GeneratorParams;
use sim::track::Track;

use super::generation::Drawn;
use super::run_log::{self, Row, Watch};
use crate::render::thumb::Thumb;

/// Window sizes the stepper goes through, in generations; `None` is the
/// whole run.
pub const WINDOWS: [Option<u32>; 5] = [Some(10), Some(20), Some(50), Some(100), None];
const DEFAULT_WINDOW: usize = 1;
/// Generated runs shown as worst.
const WORST: usize = 24;
/// Generating a track takes tens of milliseconds, so the worst are built a
/// few per frame, within this many seconds.
const BUILD_BUDGET: f64 = 0.02;

/// One library track's runs in the window.
#[derive(Debug)]
pub struct Summary {
    pub id: String,
    pub runs: usize,
    pub laps: f64,
    pub progress: f64,
    /// Share of runs ending before time was up.
    pub early: f64,
    pub common_end: String,
    /// (role, runs, mean laps), in role order.
    pub by_role: Vec<(String, usize, f64)>,
    /// Mean laps each generation over the whole run; NaN where not driven.
    pub laps_by_gen: Vec<f64>,
}

pub struct TrackResult {
    pub summary: Summary,
    pub track: Result<Arc<Track>, String>,
    pub thumb: Option<Thumb>,
}

impl TrackResult {
    pub fn name(&self) -> String {
        match &self.track {
            Ok(track) => track.def.name.clone(),
            Err(_) => self.summary.id.clone(),
        }
    }
}

pub struct RunResults {
    pub run: String,
    watch: Watch,
    rows: Vec<Row>,
    window: usize,
    pub newest: Option<u32>,
    pub library: Vec<TrackResult>,
    /// The worst generated runs; `worst` holds the built prefix.
    worst_rows: Vec<Row>,
    pub worst: Vec<Drawn>,
    generator: GeneratorParams,
    /// Built tracks by id, so polling doesn't rebuild them.
    cache: HashMap<String, Result<Arc<Track>, String>>,
}

impl RunResults {
    pub fn new(run: &str) -> Self {
        let mut results = Self {
            run: run.to_owned(),
            watch: Watch::new(run_log::run_dir(run).join("tracks.csv")),
            rows: vec![],
            window: DEFAULT_WINDOW,
            newest: None,
            library: vec![],
            worst_rows: vec![],
            worst: vec![],
            generator: run_log::generator(run),
            cache: HashMap::new(),
        };
        results.rows = run_log::parse(&results.watch.read());
        results.recompute();
        results
    }

    /// Build a few more of the worst, and re-read if the file changed.
    /// Returns true when it did.
    pub fn poll(&mut self, dt: f64) -> bool {
        let changed = self.watch.changed(dt);
        if changed {
            self.rows = run_log::parse(&self.watch.read());
            self.recompute();
        }
        let start = std::time::Instant::now();
        while self.worst.len() < self.worst_rows.len() && start.elapsed().as_secs_f64() < BUILD_BUDGET {
            let row = self.worst_rows[self.worst.len()].clone();
            let track = self.cache.entry(row.id.clone()).or_insert_with(|| run_log::build(&row.id, &self.generator)).clone();
            self.worst.push(Drawn::new(row, track));
        }
        changed
    }

    /// Worst generated tracks still being built.
    pub fn building(&self) -> usize {
        self.worst_rows.len() - self.worst.len()
    }

    pub fn window(&self) -> Option<u32> {
        WINDOWS[self.window]
    }

    /// Step the window size; returns true if it changed.
    pub fn step_window(&mut self, step: i32) -> bool {
        let at = self.window.saturating_add_signed(step as isize).min(WINDOWS.len() - 1);
        if at == self.window {
            return false;
        }
        self.window = at;
        self.recompute();
        true
    }

    /// The first generation in the window.
    pub fn first(&self) -> Option<u32> {
        Some(first_in_window(self.newest?, self.window()))
    }

    /// Runs in the window.
    pub fn runs(&self) -> usize {
        self.first().map_or(0, |first| self.rows.iter().filter(|r| r.generation >= first).count())
    }

    fn recompute(&mut self) {
        self.newest = self.rows.iter().map(|r| r.generation).max();
        let (summaries, worst) = summarise(&self.rows, self.window());
        self.worst_rows = worst.into_iter().cloned().collect();
        self.worst.clear();
        let worst = &self.worst_rows;
        self.cache.retain(|id, _| summaries.iter().any(|s| s.id == *id) || worst.iter().any(|r| r.id == *id));
        // Library tracks load from file, quickly: build them now.
        self.library = summaries
            .into_iter()
            .map(|summary| {
                let track =
                    self.cache.entry(summary.id.clone()).or_insert_with(|| run_log::build(&summary.id, &self.generator)).clone();
                TrackResult {
                    thumb: track.as_ref().ok().map(|t| Thumb::new(t)),
                    track,
                    summary,
                }
            })
            .collect();
    }
}

fn first_in_window(newest: u32, window: Option<u32>) -> u32 {
    window.map_or(0, |n| (newest + 1).saturating_sub(n))
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    if n == 0 { f64::NAN } else { sum / n as f64 }
}

/// Library tracks driven in the window, worst first (by mean laps), and the
/// generated runs in the window with the fewest laps, worst first.
pub fn summarise(rows: &[Row], window: Option<u32>) -> (Vec<Summary>, Vec<&Row>) {
    let Some(newest) = rows.iter().map(|r| r.generation).max() else { return (vec![], vec![]) };
    let first = first_in_window(newest, window);
    let recent: Vec<&Row> = rows.iter().filter(|r| r.generation >= first).collect();

    let mut ids: Vec<&str> = recent.iter().filter(|r| !r.generated()).map(|r| r.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let mut summaries: Vec<Summary> = ids
        .into_iter()
        .map(|id| {
            let runs: Vec<&Row> = recent.iter().copied().filter(|r| r.id == id).collect();
            let mut ends: Vec<(&str, usize)> = vec![];
            for r in &runs {
                match ends.iter_mut().find(|(e, _)| *e == r.end) {
                    Some((_, n)) => *n += 1,
                    None => ends.push((&r.end, 1)),
                }
            }
            let common_end = ends.iter().max_by_key(|(_, n)| *n).map_or("", |(e, _)| e).to_owned();
            let mut roles: Vec<&str> = runs.iter().map(|r| r.role.as_str()).collect();
            roles.sort_unstable_by_key(|r| role_order(r));
            roles.dedup();
            let by_role = roles
                .into_iter()
                .map(|role| {
                    let of: Vec<&&Row> = runs.iter().filter(|r| r.role == role).collect();
                    (role.to_owned(), of.len(), mean(of.iter().map(|r| r.laps)))
                })
                .collect();
            let mut per_gen = vec![(0.0, 0u32); newest as usize + 1];
            for r in rows.iter().filter(|r| r.id == id) {
                let (sum, n) = &mut per_gen[r.generation as usize];
                (*sum, *n) = (*sum + r.laps, *n + 1);
            }
            let laps_by_gen = per_gen.into_iter().map(|(sum, n)| if n == 0 { f64::NAN } else { sum / f64::from(n) }).collect();
            Summary {
                id: id.to_owned(),
                runs: runs.len(),
                laps: mean(runs.iter().map(|r| r.laps)),
                progress: mean(runs.iter().map(|r| r.progress)),
                early: runs.iter().filter(|r| r.end != "time up").count() as f64 / runs.len() as f64,
                common_end,
                by_role,
                laps_by_gen,
            }
        })
        .collect();
    summaries.sort_by(|a, b| a.laps.total_cmp(&b.laps).then_with(|| a.id.cmp(&b.id)));

    let mut worst: Vec<&Row> = recent.into_iter().filter(|r| r.generated()).collect();
    worst.sort_by(|a, b| a.laps.total_cmp(&b.laps).then(a.progress.total_cmp(&b.progress)));
    let mut seen: Vec<&str> = vec![];
    worst.retain(|r| {
        let new = !seen.contains(&r.id.as_str());
        seen.push(&r.id);
        new
    });
    worst.truncate(WORST);
    (summaries, worst)
}

fn role_order(role: &str) -> usize {
    ["fixed", "pool", "check", "test", "generated"].iter().position(|r| *r == role).unwrap_or(5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(generation: u32, id: &str, role: &str, laps: f64, end: &str) -> Row {
        Row { generation, id: id.into(), role: role.into(), progress: laps * 1000.0, laps, end: end.into() }
    }

    #[test]
    fn library_tracks_in_the_window_worst_first() {
        let rows = vec![
            row(0, "tracks/a.toml", "pool", 0.0, "crashed"),
            row(8, "tracks/a.toml", "pool", 0.9, "time up"),
            row(9, "tracks/a.toml", "check", 0.7, "crashed"),
            row(9, "tracks/b.toml", "fixed", 0.2, "crashed"),
            row(1, "tracks/c.toml", "pool", 0.1, "crashed"),
        ];
        let (s, _) = summarise(&rows, Some(5));
        // c only ran before the window.
        assert_eq!(s.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["tracks/b.toml", "tracks/a.toml"]);
        let a = &s[1];
        assert_eq!(a.runs, 2);
        assert!((a.laps - 0.8).abs() < 1e-9);
        assert!((a.early - 0.5).abs() < 1e-9);
        assert_eq!(a.by_role, vec![("pool".to_owned(), 1, 0.9), ("check".to_owned(), 1, 0.7)]);
        // The sparkline covers the whole run, with gaps.
        assert_eq!(a.laps_by_gen.len(), 10);
        assert_eq!(a.laps_by_gen[0], 0.0);
        assert!(a.laps_by_gen[1].is_nan());
        assert!((a.laps_by_gen[9] - 0.7).abs() < 1e-9);
    }

    #[test]
    fn most_common_end() {
        let rows = vec![
            row(0, "tracks/a.toml", "pool", 0.1, "stalled"),
            row(1, "tracks/a.toml", "pool", 0.1, "crashed"),
            row(2, "tracks/a.toml", "pool", 0.1, "crashed"),
        ];
        let (s, _) = summarise(&rows, None);
        assert_eq!(s[0].common_end, "crashed");
        assert!((s[0].early - 1.0).abs() < 1e-9);
    }

    #[test]
    fn worst_generated_are_generated_runs_in_the_window_fewest_laps_first() {
        let mut rows = vec![
            row(0, "gen:1", "generated", 0.0, "crashed"),
            row(9, "gen:2", "generated", 0.5, "time up"),
            row(9, "gen:3", "check", 0.1, "crashed"),
            row(9, "tracks/a.toml", "pool", 0.0, "crashed"),
        ];
        for i in 0..30 {
            rows.push(row(8, &format!("gen:{}", 100 + i), "generated", 0.6 + f64::from(i) * 0.01, "time up"));
        }
        let (_, worst) = summarise(&rows, Some(5));
        assert_eq!(worst.len(), WORST);
        assert_eq!(worst[0].id, "gen:3");
        assert_eq!(worst[1].id, "gen:2");
        assert!(worst.iter().all(|r| r.generated() && r.generation >= 5));
    }
}

