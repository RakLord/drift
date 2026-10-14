//! The headline numbers under the dashboard's title: this stage's best,
//! where its trend is heading, generation time, the saved driver, and
//! whether the trainer is still writing.

use std::time::Duration;

use macroquad::prelude::*;

use super::gate::number;
use super::metrics::Metrics;
use super::trend::rolling_mean;
use crate::render::{text, theme};

/// The best's trend now is compared with this many generations ago...
const TREND_SPAN: usize = 100;
/// ...each a mean over this many.
const TREND_WINDOW: usize = 20;
/// Generations averaged for the generation time.
const RECENT: usize = 10;

/// One row of `label value` pairs. Returns its width.
pub fn draw(m: &Metrics, x: f32, y: f32) -> f32 {
    let mut items: Vec<(&str, String, Color)> = vec![];
    let last = m.len() - 1;
    let best = m.series("best");

    // Fitness is shaped differently per stage, so only this stage's count.
    let stage = m.text("stage", last);
    let this_stage = |row: usize| m.text("stage", row) == stage;
    let peak = (0..m.len()).filter(|&r| this_stage(r) && best[r].is_finite()).max_by(|&a, &b| best[a].total_cmp(&best[b]));
    if let Some(row) = peak {
        items.push(("best this stage", format!("{} (gen {})", number(best[row]), m.text("generation", row)), theme::TEXT));
    }

    let trend = rolling_mean(&best, TREND_WINDOW);
    if let Some(then) = last.checked_sub(TREND_SPAN).filter(|&r| r + 1 >= TREND_WINDOW && this_stage(r))
        && trend[last].is_finite()
        && trend[then].is_finite()
    {
        let change = trend[last] - trend[then];
        let (sign, colour) = if change >= 0.0 { ("+", theme::GRIP) } else { ("-", theme::SATURATED) };
        items.push(("best trend", format!("{sign}{} over {TREND_SPAN} gens", number(change.abs())), colour));
    }

    let recent: Vec<f64> = m.series("seconds").into_iter().rev().take(RECENT).filter(|v| v.is_finite()).collect();
    let gen_secs = (!recent.is_empty()).then(|| recent.iter().sum::<f64>() / recent.len() as f64);
    if let Some(secs) = gen_secs.filter(|&s| s > 0.0) {
        items.push(("gen time", format!("{secs:.1}s ({:.0}/h)", 3600.0 / secs), theme::TEXT));
    }

    let (saved, rank) = (m.series("saved_fitness")[last], m.series("saved_rank")[last]);
    if saved.is_finite() && rank.is_finite() {
        items.push(("saved driver", format!("{} (rank {rank:.0})", number(saved)), theme::TEXT));
    }

    if let Some(age) = m.age() {
        // Quiet for three generations' time (and at least a minute).
        let quiet = gen_secs.map_or(120.0, |s| (3.0 * s).max(60.0));
        items.push(if age.as_secs_f64() > quiet {
            ("no new gen for", duration(age), theme::SODIUM)
        } else {
            ("last gen", format!("{} ago", duration(age)), theme::TEXT)
        });
    }

    let mut cx = x;
    for (label, value, colour) in items {
        cx += text(&format!("{label}  "), cx, y, 18.0, theme::TEXT_DIM);
        cx += text(&value, cx, y, 18.0, colour);
        cx += 28.0;
    }
    cx - x
}

fn duration(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else if s < 86_400 {
        format!("{}h {}m", s / 3600, s % 3600 / 60)
    } else {
        format!("{}d {}h", s / 86_400, s % 86_400 / 3600)
    }
}
