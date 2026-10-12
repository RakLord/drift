//! Run dashboard (C): the followed run's metrics.csv as small multiples,
//! updating live as the trainer writes. Hover any chart for that
//! generation's values; dashed lines mark curriculum stage changes. A meter
//! under the title shows the saved driver against the current stage's gate.

mod chart;
mod gate;
mod metrics;

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::render::{text, theme};
use chart::{Chart, Kind, Series, tooltip};
use gate::Gate;
use metrics::Metrics;

pub struct Dashboard {
    pub open: bool,
    metrics: Option<Metrics>,
    /// Generation row under the mouse last frame (shared by all charts).
    hover: Option<usize>,
    gate: Option<Gate>,
    /// Rows in metrics when `gate` was last read.
    gate_rows: usize,
}

impl Dashboard {
    /// `metrics_csv`: the run being watched, if any.
    pub fn new(metrics_csv: Option<PathBuf>, open: bool) -> Self {
        Self { open, metrics: metrics_csv.map(Metrics::new), hover: None, gate: None, gate_rows: 0 }
    }

    pub fn update(&mut self, dt: f64) {
        if let Some(m) = &mut self.metrics {
            m.poll(dt);
            // Re-read each generation: the stage may have moved on, or the
            // run's config been edited for a --resume.
            if m.len() != self.gate_rows {
                self.gate_rows = m.len();
                let config = m.path.with_file_name("config").join("training.toml");
                self.gate = m.len().checked_sub(1).and_then(|last| Gate::load(&config, m.text("stage", last)));
            }
        }
    }

    pub fn draw(&mut self) {
        if !self.open {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, Color { a: 0.92, ..theme::ASPHALT });
        let Some(m) = &self.metrics else {
            centred("No run to chart: start the viewer with --follow <run> (C to close)", w, h);
            return;
        };
        if m.len() == 0 {
            centred(&format!("Waiting for {} (C to close)", m.path.display()), w, h);
            return;
        }

        let last = m.len() - 1;
        let title = format!(
            "RUN DASHBOARD   {}   gen {}   stage {}   (C to close)",
            m.path.parent().map_or(String::new(), |p| p.display().to_string()),
            m.text("generation", last),
            m.text("stage", last)
        );
        text(&title, 20.0, 30.0, 22.0, theme::NEON_CYAN);
        let top = match &self.gate {
            Some(gate) => {
                gate.draw(m, 20.0, 58.0);
                76.0
            }
            None => 48.0,
        };

        let percent = |column: &str| m.series(column).into_iter().map(|v| v * 100.0).collect::<Vec<_>>();
        let generations = m.series("generation");
        let (best, mean, test) = (m.series("best"), m.series("mean"), m.series("test_fitness"));
        let (top_q, best_q, mean_q) = (m.series("top_quality"), m.series("best_quality"), m.series("mean_quality"));
        let above_20 = percent("above_20_share");
        let ends: Vec<(&str, Vec<f64>)> = [
            ("time up", "end_time_up"),
            ("no progress", "end_no_progress"),
            ("crashed", "end_crashed"),
            ("stalled", "end_stalled"),
            ("spun", "end_spun"),
            ("backwards", "end_backwards"),
        ]
        .iter()
        .map(|&(name, column)| (name, m.series(column)))
        .collect();
        let (handbrake, clutch, slip) = (percent("handbrake_share"), percent("clutch_share"), percent("clutch_slip"));
        let diversity = m.series("behaviour_diversity");

        let line = |name, values| Series { name, values, dots: false };
        let charts = [
            Chart {
                title: "Fitness",
                unit: "",
                series: vec![line("best", &best), line("mean", &mean), Series { name: "test tracks", values: &test, dots: true }],
                kind: Kind::Lines,
                range: None,
            },
            Chart {
                title: "Drift quality (1 = 45° held all run)",
                unit: "",
                series: vec![line("top drifter", &top_q), line("fittest", &best_q), line("population", &mean_q)],
                kind: Kind::Lines,
                range: None,
            },
            Chart { title: "Time above 20°", unit: "%", series: vec![line("population", &above_20)], kind: Kind::Lines, range: None },
            Chart {
                title: "How runs end",
                unit: "%",
                series: ends.iter().map(|(name, values)| line(name, values)).collect(),
                kind: Kind::Stacked,
                range: Some((0.0, 100.0)),
            },
            Chart {
                title: "Pedal use",
                unit: "%",
                series: vec![line("handbrake", &handbrake), line("clutch", &clutch), line("clutch slipping", &slip)],
                kind: Kind::Lines,
                range: Some((0.0, 100.0)),
            },
            Chart {
                title: "Behaviour diversity",
                unit: "",
                series: vec![line("population", &diversity)],
                kind: Kind::Lines,
                range: None,
            },
        ];

        // 3 x 2 when there's room, 2 x 3 when narrow.
        let columns = if w > 1400.0 { 3 } else { 2 };
        let rows = charts.len().div_ceil(columns);
        let (gap, margin) = (12.0, 16.0);
        let cell_w = (w - 2.0 * margin - gap * (columns - 1) as f32) / columns as f32;
        let cell_h = (h - top - margin - gap * (rows - 1) as f32) / rows as f32;

        let stages = m.stage_changes();
        let mut hovered = None;
        for (i, chart) in charts.iter().enumerate() {
            let (c, r) = (i % columns, i / columns);
            let rect = Rect::new(margin + c as f32 * (cell_w + gap), top + r as f32 * (cell_h + gap), cell_w, cell_h);
            let frame = chart.draw(rect, &stages, self.hover, &generations);
            if let Some(row) = frame.hovered() {
                hovered = Some((i, row));
            }
        }
        self.hover = hovered.map(|(_, row)| row);
        if let Some((i, row)) = hovered {
            let heading = format!("gen {}  ·  {}", m.text("generation", row), m.text("stage", row));
            tooltip(&heading, &charts[i].values_at(row));
        }
    }
}

fn centred(message: &str, w: f32, h: f32) {
    let width = measure_text(message, None, 22, 1.0).width;
    text(message, (w - width) / 2.0, h / 2.0, 22.0, theme::TEXT);
}
