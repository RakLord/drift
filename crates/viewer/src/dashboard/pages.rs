//! The dashboard's pages: which charts each shows, from which metrics.csv
//! columns. Columns a run never logged leave their series out.

use super::chart::{Chart, Kind, Series};
use super::metrics::Metrics;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Scoring,
    Driving,
    Health,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Overview, Page::Scoring, Page::Driving, Page::Health];

    pub fn name(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Scoring => "Scoring",
            Page::Driving => "Driving",
            Page::Health => "Health",
        }
    }

    pub fn charts(self, m: &Metrics) -> Vec<Chart> {
        match self {
            Page::Overview => overview(m),
            Page::Scoring => scoring(m),
            Page::Driving => driving(m),
            Page::Health => health(m),
        }
    }
}

fn chart(title: &'static str, unit: &'static str, series: Vec<Series>) -> Chart {
    Chart { title, unit, series, kind: Kind::Lines, range: None }
}

fn line(m: &Metrics, name: &str, column: &str) -> Series {
    Series::line(name, m.series(column))
}

/// A 0..1 share column as a percentage.
fn percent(m: &Metrics, name: &str, column: &str) -> Series {
    Series::line(name, m.series(column).into_iter().map(|v| v * 100.0).collect())
}

fn overview(m: &Metrics) -> Vec<Chart> {
    let ends = [
        ("time up", "end_time_up"),
        ("no progress", "end_no_progress"),
        ("crashed", "end_crashed"),
        ("stalled", "end_stalled"),
        ("spun", "end_spun"),
        ("backwards", "end_backwards"),
    ];
    vec![
        chart(
            "Fitness",
            "",
            vec![line(m, "best", "best"), line(m, "mean", "mean"), Series::dots("test tracks", m.series("test_fitness"))],
        ),
        chart(
            "Drift quality (flow: 1 = full-angle drift, sides held, all run)",
            "",
            vec![line(m, "top drifter", "top_quality"), line(m, "fittest", "best_quality"), line(m, "population", "mean_quality")],
        ),
        chart("Time above 20°", "%", vec![percent(m, "population", "above_20_share")]),
        Chart {
            title: "How runs end",
            unit: "%",
            series: ends.iter().map(|&(name, column)| line(m, name, column)).collect(),
            kind: Kind::Stacked,
            range: Some((0.0, 100.0)),
        },
        chart(
            "Pedal use",
            "%",
            vec![
                percent(m, "handbrake", "handbrake_share"),
                percent(m, "clutch", "clutch_share"),
                percent(m, "clutch slipping", "clutch_slip"),
            ],
        ),
        chart("Behaviour diversity", "", vec![line(m, "population", "behaviour_diversity")]),
    ]
}

fn scoring(m: &Metrics) -> Vec<Chart> {
    let parts = |who: &str| ["line", "angle", "style", "penalty"].map(|part| line(m, part, &format!("{who}_{part}"))).into();
    vec![
        chart("Saved driver's score (mean per run)", "", parts("best")),
        chart("Population's score (mean per run)", "", parts("mean")),
        chart("Saved driver's judged objective", "", vec![line(m, "saved driver", "best_judged")]),
        chart("Top drifter's judged score", "", vec![line(m, "top drifter", "top_score")]),
        chart(
            "Average drift angle",
            "°",
            vec![line(m, "top drifter", "top_angle"), line(m, "population", "drift_angle")],
        ),
        chart(
            "Seconds per drift side",
            "s",
            vec![line(m, "top drifter", "top_hold"), line(m, "population", "drift_hold")],
        ),
    ]
}

fn driving(m: &Metrics) -> Vec<Chart> {
    vec![
        chart(
            "Progress per run",
            "m",
            vec![line(m, "saved driver", "best_progress"), line(m, "population", "mean_progress")],
        ),
        chart(
            "Laps per run",
            "",
            vec![
                line(m, "saved driver", "best_laps"),
                line(m, "top drifter", "top_laps"),
                Series::dots("test tracks", m.series("test_laps")),
            ],
        ),
        chart(
            "Biggest drift angle per run",
            "°",
            vec![line(m, "median", "max_angle_p50"), line(m, "90th percentile", "max_angle_p90")],
        ),
        chart(
            "Sawing while drifting (per second)",
            "",
            vec![
                line(m, "throttle, saved", "best_throttle_saws"),
                line(m, "steer, saved", "best_steer_saws"),
                line(m, "throttle, population", "mean_throttle_saws"),
                line(m, "steer, population", "mean_steer_saws"),
            ],
        ),
        chart("Spins per run", "", vec![line(m, "population", "spins")]),
        chart("Zones and clips per run", "", vec![line(m, "population", "zones")]),
    ]
}

fn health(m: &Metrics) -> Vec<Chart> {
    let islands = m
        .columns()
        .iter()
        .filter_map(|c| {
            let n = c.strip_prefix("island")?.strip_suffix("_best")?;
            Some(line(m, &format!("island {n}"), c))
        })
        .collect();
    let steps = m.series("steps_per_sec").into_iter().map(|v| v / 1e6).collect();
    vec![
        chart("Best per island", "", islands),
        Chart {
            title: "Fitness spread (worst to best)",
            unit: "",
            series: vec![line(m, "worst", "worst"), line(m, "best", "best"), line(m, "median", "median")],
            kind: Kind::Band,
            range: None,
        },
        chart("Weight diversity", "", vec![line(m, "population", "diversity")]),
        chart("Mutation sigma", "", vec![line(m, "population", "sigma")]),
        chart("Seconds per generation", "s", vec![line(m, "generation", "seconds")]),
        chart("Sim speed (million steps/s)", "", vec![Series::line("trainer", steps)]),
    ]
}
