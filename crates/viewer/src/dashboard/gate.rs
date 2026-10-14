//! The current stage's advance gate, read from the run's saved
//! config/training.toml, and how close the saved driver is to passing it.

use std::path::Path;

use macroquad::prelude::*;

use super::metrics::Metrics;
use crate::render::{text, theme};

pub struct Gate {
    stage: String,
    metric: String,
    at_least: f64,
    generations: u32,
}

impl Gate {
    /// The gate out of `stage`, if it has one and isn't the last stage
    /// (`training_toml`: the run's config snapshot).
    pub fn load(training_toml: &Path, stage: &str) -> Option<Self> {
        let config: toml::Value = toml::from_str(&std::fs::read_to_string(training_toml).ok()?).ok()?;
        let stages = config.get("curriculum")?.as_array()?;
        let k = stages.iter().position(|s| s.get("name").and_then(|n| n.as_str()) == Some(stage))?;
        if k + 1 >= stages.len() {
            return None;
        }
        let advance = stages[k].get("advance")?;
        Some(Self {
            stage: stage.to_owned(),
            metric: advance.get("metric")?.as_str()?.to_owned(),
            at_least: advance.get("at_least").and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)))?,
            generations: u32::try_from(advance.get("generations")?.as_integer()?).ok()?,
        })
    }

    /// The gated value per generation: the saved driver's runs, as the
    /// trainer measures it (evo::fitness::Metric).
    fn values(&self, m: &Metrics) -> Vec<f64> {
        let column = |name: &str| m.series(name);
        match self.metric.as_str() {
            "progress" => column("best_progress"),
            "angle" => column("best_angle"),
            "line" => column("best_line"),
            "style" => column("best_style"),
            "drift_quality" => column("best_quality"),
            "score" => {
                let (line, angle, style, penalty) =
                    (column("best_line"), column("best_angle"), column("best_style"), column("best_penalty"));
                (0..line.len()).map(|i| line[i] + angle[i] + style[i] - penalty[i]).collect()
            }
            _ => vec![f64::NAN; m.len()],
        }
    }

    /// One row: label, a bar filled to the rolling mean with a tick at the
    /// gate, how full the window is, and the latest value. Returns its width.
    pub fn draw(&self, m: &Metrics, x: f32, y: f32) -> f32 {
        let values = self.values(m);
        let latest = values.last().copied().unwrap_or(f64::NAN);
        let window = self.generations.max(1) as usize;
        let (average, count) = rolling(&values, |i| m.text("stage", i) == self.stage, window);
        let passing = count == window && average >= self.at_least;
        let unit = if self.metric == "progress" { " m" } else { "" };

        let mut cx = x;
        cx += text(&format!("gate  {}  avg ", self.metric), cx, y, 18.0, theme::TEXT_DIM);
        cx += text(&format!("{} / {}{unit}", number(average), number(self.at_least)), cx, y, 18.0, theme::TEXT);
        cx += 12.0;

        // Bar spans 0..1.25 x the gate, so the tick sits at 80%.
        let (bar_w, bar_h) = (180.0, 10.0);
        let top = y - bar_h;
        let span = self.at_least * 1.25;
        let fill = if span > 0.0 && average.is_finite() { (average / span).clamp(0.0, 1.0) as f32 } else { 0.0 };
        draw_rectangle(cx, top, bar_w, bar_h, theme::PANEL_SOLID);
        draw_rectangle(cx, top, bar_w * fill, bar_h, if passing { theme::GRIP } else { theme::SODIUM });
        draw_rectangle_lines(cx, top, bar_w, bar_h, 1.0, theme::GRID_MAJOR);
        let tick = cx + bar_w * 0.8;
        draw_line(tick, top - 3.0, tick, top + bar_h + 3.0, 2.0, theme::TEXT);
        cx += bar_w + 12.0;

        cx += text(&format!("over {count}/{window} gens  latest {}", number(latest)), cx, y, 18.0, theme::TEXT_DIM);
        cx - x
    }
}

/// Mean of the newest `window` values whose rows are `in_stage` (stopping at
/// the first row that isn't), and how many it averaged. The trainer's window
/// starts empty on --resume, so after a resume this can run ahead of it.
fn rolling(values: &[f64], in_stage: impl Fn(usize) -> bool, window: usize) -> (f64, usize) {
    let recent: Vec<f64> = (0..values.len()).rev().take_while(|&i| in_stage(i)).take(window).map(|i| values[i]).collect();
    if recent.is_empty() { (f64::NAN, 0) } else { (recent.iter().sum::<f64>() / recent.len() as f64, recent.len()) }
}

pub(super) fn number(v: f64) -> String {
    if !v.is_finite() {
        "-".to_owned()
    } else if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_gate_out_of_a_stage_but_not_the_last() {
        let dir = std::env::temp_dir().join(format!("drift-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("training.toml");
        let toml = "[[curriculum]]\nname = \"drive\"\n\n[curriculum.advance]\nmetric = \"progress\"\nat_least = 750.0\ngenerations = 10\n\n\
                    [[curriculum]]\nname = \"slide\"\n\n[curriculum.advance]\nmetric = \"drift_quality\"\nat_least = 0.3\ngenerations = 5\n";
        std::fs::write(&path, toml).unwrap();

        let gate = Gate::load(&path, "drive").unwrap();
        assert_eq!((gate.metric.as_str(), gate.at_least, gate.generations), ("progress", 750.0, 10));
        assert!(Gate::load(&path, "slide").is_none());
        assert!(Gate::load(&path, "missing").is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rolling_mean_covers_only_the_current_stage() {
        let values = [9.0, 1.0, 2.0, 3.0];
        // Row 0 is an earlier stage.
        assert_eq!(rolling(&values, |i| i > 0, 2), (2.5, 2));
        assert_eq!(rolling(&values, |i| i > 0, 10), (2.0, 3));
        assert!(rolling(&values, |_| false, 3).0.is_nan());
    }
}
