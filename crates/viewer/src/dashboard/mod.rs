//! Run dashboard (C): the followed run's metrics.csv as pages of small
//! multiples (1-4 or click a tab), updating live as the trainer writes.
//! Legends read the latest generation, or the one hovered or pinned (click
//! a chart); the tooltip adds each series' trend and its change. Bold trend
//! lines run over faint raw values (S cycles), Z narrows to the latest
//! generations, dashed lines mark curriculum stage changes. Under the
//! title: headline numbers, and the saved driver against the stage's gate.
//! While open it takes the keys and mouse; the screen under it waits.

pub mod chart;
mod gate;
mod kpi;
mod metrics;
mod pages;
mod trend;

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::render::{text, theme};
use chart::{LineStyle, View, tooltip};
use gate::Gate;
use metrics::Metrics;
use pages::Page;

/// Z cycles through these: the whole run, then the latest generations.
const WINDOWS: [Option<usize>; 3] = [None, Some(500), Some(100)];
const PAGE_KEYS: [KeyCode; 4] = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4];
/// Room for the key help along the bottom.
const FOOTER: f32 = 30.0;

pub struct Dashboard {
    pub open: bool,
    metrics: Option<Metrics>,
    /// Generation row under the mouse last frame (shared by all charts).
    hover: Option<usize>,
    /// Row pinned by a click: legends and a crosshair stay on it.
    pinned: Option<usize>,
    page: Page,
    /// Index into `WINDOWS`.
    window: usize,
    style: LineStyle,
    gate: Option<Gate>,
    /// Rows in metrics when `gate` was last read.
    gate_rows: usize,
}

impl Dashboard {
    /// `metrics_csv`: the run being watched, if any.
    pub fn new(metrics_csv: Option<PathBuf>, open: bool) -> Self {
        Self {
            open,
            metrics: metrics_csv.map(Metrics::new),
            hover: None,
            pinned: None,
            page: Page::Overview,
            window: 0,
            style: LineStyle::Both,
            gate: None,
            gate_rows: 0,
        }
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
        if !self.open {
            return;
        }
        for (key, page) in PAGE_KEYS.into_iter().zip(Page::ALL) {
            if is_key_pressed(key) {
                self.page = page;
            }
        }
        if is_key_pressed(KeyCode::Z) {
            self.window = (self.window + 1) % WINDOWS.len();
        }
        if is_key_pressed(KeyCode::S) {
            self.style = self.style.next();
        }
    }

    pub fn draw(&mut self) {
        if !self.open {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, theme::ASPHALT);
        let Some(m) = &self.metrics else {
            centred("No run to chart: start the viewer with --follow <run> (C to close)", w, h);
            return;
        };
        if m.len() == 0 {
            centred(&format!("Waiting for {} (C to close)", m.path.display()), w, h);
            return;
        }

        let len = m.len();
        let title = format!(
            "RUN DASHBOARD   {}   gen {}   stage {}",
            m.path.parent().map_or(String::new(), |p| p.display().to_string()),
            m.text("generation", len - 1),
            m.text("stage", len - 1)
        );
        text(&title, 20.0, 30.0, 22.0, theme::NEON_CYAN);
        if let Some(page) = tabs(self.page, w - 20.0, 30.0) {
            self.page = page;
        }
        kpi::draw(m, 20.0, 58.0);
        let top = match &self.gate {
            Some(gate) => {
                gate.draw(m, 20.0, 84.0);
                102.0
            }
            None => 76.0,
        };

        let rows = match WINDOWS[self.window] {
            Some(n) => len.saturating_sub(n)..len,
            None => 0..len,
        };
        let generations = m.series("generation");
        let stages = m.stage_changes();
        let view = View {
            trend: trend::window_for(rows.len()),
            rows,
            style: self.style,
            stages: &stages,
            generations: &generations,
            hover: self.hover,
            pinned: self.pinned.filter(|&r| r < len),
        };

        // 3 x 2 when there's room, 2 x 3 when narrow.
        let charts = self.page.charts(m);
        let columns = if w > 1400.0 { 3 } else { 2 };
        let grid_rows = charts.len().div_ceil(columns);
        let (gap, margin) = (12.0, 16.0);
        let cell_w = (w - 2.0 * margin - gap * (columns - 1) as f32) / columns as f32;
        let cell_h = (h - top - FOOTER - gap * (grid_rows - 1) as f32) / grid_rows as f32;
        let mut hovered = None;
        for (i, chart) in charts.iter().enumerate() {
            let (c, r) = (i % columns, i / columns);
            let rect = Rect::new(margin + c as f32 * (cell_w + gap), top + r as f32 * (cell_h + gap), cell_w, cell_h);
            if let Some(row) = chart.draw(rect, &view).hovered() {
                hovered = Some((i, row));
            }
        }
        self.hover = hovered.map(|(_, row)| row);
        if is_mouse_button_pressed(MouseButton::Left) && self.hover.is_some() {
            self.pinned = if self.pinned.is_some() { None } else { self.hover };
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            self.pinned = None;
        }

        let pin = match view.pinned {
            Some(row) => format!("pinned gen {} (click to clear)", m.text("generation", row)),
            None => "click a chart: pin a gen".to_owned(),
        };
        let window = match WINDOWS[self.window] {
            Some(n) => format!("last {n} gens"),
            None => "all gens".to_owned(),
        };
        let lines = match view.trend {
            Some(n) => format!("{} ({n}-gen mean)", self.style.label()),
            None => "raw (too few gens for a trend)".to_owned(),
        };
        let help = format!("1-4  page      Z  {window}      S  {lines}      {pin}      C  close");
        text(&help, 20.0, h - 10.0, 16.0, theme::TEXT_DIM);

        if let Some((i, row)) = hovered {
            let mut heading = format!("gen {}  ·  {}", m.text("generation", row), m.text("stage", row));
            if let Some(n) = view.trend {
                heading += &format!("  ·  trend: {n}-gen mean (change over {n})");
            }
            tooltip(&heading, &charts[i].values_at(row, &view));
        }
    }
}

/// Page tabs, right-aligned to `right` on the title row. Returns the one
/// clicked, if any.
fn tabs(current: Page, right: f32, y: f32) -> Option<Page> {
    const SIZE: u16 = 18;
    const GAP: f32 = 24.0;
    let labels: Vec<String> = Page::ALL.iter().enumerate().map(|(i, p)| format!("{} {}", i + 1, p.name())).collect();
    let widths: Vec<f32> = labels.iter().map(|l| measure_text(l, None, SIZE, 1.0).width).collect();
    let mut x = right - widths.iter().sum::<f32>() - GAP * (labels.len() - 1) as f32;
    let (mx, my) = mouse_position();
    let mut clicked = None;
    for ((page, label), width) in Page::ALL.into_iter().zip(&labels).zip(widths) {
        let hovered = mx >= x && mx <= x + width && my >= y - 20.0 && my <= y + 8.0;
        let colour = if page == current {
            theme::NEON_CYAN
        } else if hovered {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        text(label, x, y, f32::from(SIZE), colour);
        if page == current {
            draw_line(x, y + 6.0, x + width, y + 6.0, 2.0, theme::NEON_CYAN);
        }
        if hovered && is_mouse_button_pressed(MouseButton::Left) {
            clicked = Some(page);
        }
        x += width + GAP;
    }
    clicked
}

fn centred(message: &str, w: f32, h: f32) {
    let width = measure_text(message, None, 22, 1.0).width;
    text(message, (w - width) / 2.0, h / 2.0, 22.0, theme::TEXT);
}
