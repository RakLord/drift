//! Small multiples for run metrics: one y-axis per chart, thin lines, a
//! recessive grid, a legend for two or more series, stage changes marked,
//! and a shared hover crosshair.

use macroquad::prelude::*;

use crate::render::{text, theme};

/// Categorical series colours in fixed order (validated for the dark panel:
/// lightness band, CVD separation, >= 3:1 contrast).
pub const SERIES: [Color; 6] = [
    rgb(0x39, 0x87, 0xe5), // blue
    rgb(0xd9, 0x59, 0x26), // orange
    rgb(0x19, 0x9e, 0x70), // aqua
    rgb(0xc9, 0x85, 0x00), // yellow
    rgb(0xd5, 0x51, 0x81), // magenta
    rgb(0x00, 0x83, 0x00), // green
];

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

pub struct Series<'a> {
    pub name: &'a str,
    pub values: &'a [f64],
    /// Draw as dots (sparse values, e.g. test results) instead of a line.
    pub dots: bool,
}

pub enum Kind {
    Lines,
    /// Shares stacked to 100%.
    Stacked,
}

pub struct Chart<'a> {
    pub title: &'a str,
    pub unit: &'a str,
    pub series: Vec<Series<'a>>,
    pub kind: Kind,
    /// Fixed y-range, or None to fit the data.
    pub range: Option<(f64, f64)>,
}

pub struct Frame {
    pub rect: Rect,
    plot: Rect,
    lo: f64,
    hi: f64,
    count: usize,
}

const PAD_LEFT: f32 = 52.0;
/// Below the title; legend rows add to this.
const PAD_TOP: f32 = 40.0;
const LEGEND_ROW: f32 = 18.0;
const PAD_BOTTOM: f32 = 22.0;
const PAD_RIGHT: f32 = 14.0;

impl Frame {
    fn x(&self, i: usize) -> f32 {
        let span = (self.count.max(2) - 1) as f32;
        self.plot.x + self.plot.w * i as f32 / span
    }

    fn y(&self, v: f64) -> f32 {
        let t = ((v - self.lo) / (self.hi - self.lo).max(1e-9)) as f32;
        self.plot.y + self.plot.h * (1.0 - t.clamp(0.0, 1.0))
    }

    /// Row under the mouse, if it's over this chart's plot.
    pub fn hovered(&self) -> Option<usize> {
        let (mx, my) = mouse_position();
        let inside = mx >= self.plot.x && mx <= self.plot.right() && my >= self.rect.y && my <= self.rect.bottom();
        if !inside || self.count == 0 {
            return None;
        }
        let t = (mx - self.plot.x) / self.plot.w;
        Some(((t * (self.count.max(2) - 1) as f32).round() as usize).min(self.count - 1))
    }
}

/// "Nice" tick step for a range.
fn tick_step(span: f64) -> f64 {
    let raw = span / 4.0;
    let mag = 10f64.powf(raw.abs().max(1e-9).log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag)
}

fn format_tick(v: f64, step: f64) -> String {
    if step >= 1.0 { format!("{v:.0}") } else if step >= 0.1 { format!("{v:.1}") } else { format!("{v:.2}") }
}

impl Chart<'_> {
    fn fit(&self) -> (f64, f64) {
        if let Some(r) = self.range {
            return r;
        }
        let finite = self.series.iter().flat_map(|s| s.values.iter().copied()).filter(|v| v.is_finite());
        let (lo, hi) = finite.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        if lo > hi {
            return (0.0, 1.0);
        }
        let lo = lo.min(0.0);
        let hi = if hi - lo < 1e-9 { lo + 1.0 } else { hi };
        let step = tick_step(hi - lo);
        ((lo / step).floor() * step, (hi / step).ceil() * step)
    }

    pub fn draw(&self, rect: Rect, stages: &[(usize, String)], hover: Option<usize>, generations: &[f64]) -> Frame {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, theme::PANEL_SOLID);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, theme::GRID_MAJOR);
        text(self.title, rect.x + 12.0, rect.y + 22.0, 20.0, theme::TEXT);

        let count = self.series.first().map_or(0, |s| s.values.len());
        let (lo, hi) = self.fit();

        // Legend (two or more series) on its own rows under the title,
        // wrapping as needed: coloured swatch, text in text colour.
        let mut legend_rows = 0.0;
        if self.series.len() >= 2 {
            legend_rows = 1.0;
            let (left, right) = (rect.x + 12.0, rect.right() - 12.0);
            let mut x = left;
            for (k, s) in self.series.iter().enumerate() {
                let w = 14.0 + measure_text(s.name, None, 14, 1.0).width;
                if x + w > right && x > left {
                    x = left;
                    legend_rows += 1.0;
                }
                let y = rect.y + 24.0 + legend_rows * LEGEND_ROW;
                draw_rectangle(x, y - 10.0, 10.0, 10.0, SERIES[k]);
                text(s.name, x + 14.0, y, 14.0, theme::TEXT_DIM);
                x += w + 14.0;
            }
        }
        let top = PAD_TOP + legend_rows * LEGEND_ROW;
        let plot = Rect::new(rect.x + PAD_LEFT, rect.y + top, rect.w - PAD_LEFT - PAD_RIGHT, rect.h - top - PAD_BOTTOM);
        let frame = Frame { rect, plot, lo, hi, count };

        // Recessive grid and y ticks.
        let step = tick_step(hi - lo);
        let mut v = lo;
        while v <= hi + step * 0.01 {
            let y = frame.y(v);
            draw_line(plot.x, y, plot.right(), y, 1.0, theme::GRID_MINOR);
            let label = format!("{}{}", format_tick(v, step), self.unit);
            let w = measure_text(&label, None, 13, 1.0).width;
            text(&label, plot.x - w - 6.0, y + 4.0, 13.0, theme::TEXT_DIM);
            v += step;
        }
        // x: first and last generation.
        if count > 0 {
            let first = format!("gen {:.0}", generations.first().copied().unwrap_or(0.0));
            let last = format!("{:.0}", generations.last().copied().unwrap_or(0.0));
            text(&first, plot.x, rect.bottom() - 6.0, 13.0, theme::TEXT_DIM);
            let w = measure_text(&last, None, 13, 1.0).width;
            text(&last, plot.right() - w, rect.bottom() - 6.0, 13.0, theme::TEXT_DIM);
        }

        // Stage changes.
        for (row, name) in stages.iter().filter(|(row, _)| *row > 0) {
            let x = frame.x(*row);
            let mut y = plot.y;
            while y < plot.bottom() {
                draw_line(x, y, x, (y + 4.0).min(plot.bottom()), 1.0, theme::TEXT_DIM);
                y += 8.0;
            }
            text(name, x + 4.0, plot.y + 12.0, 13.0, theme::TEXT_DIM);
        }

        match self.kind {
            Kind::Lines => self.draw_lines(&frame),
            Kind::Stacked => self.draw_stacked(&frame),
        }

        if let Some(row) = hover.filter(|&r| r < count) {
            let x = frame.x(row);
            draw_line(x, plot.y, x, plot.bottom(), 1.0, theme::TEXT_DIM);
            if let Kind::Lines = self.kind {
                for (k, s) in self.series.iter().enumerate() {
                    if let Some(&v) = s.values.get(row).filter(|v| v.is_finite()) {
                        draw_circle(x, frame.y(v), 5.0, theme::PANEL_SOLID);
                        draw_circle(x, frame.y(v), 4.0, SERIES[k]);
                    }
                }
            }
        }
        frame
    }

    fn draw_lines(&self, f: &Frame) {
        for (k, s) in self.series.iter().enumerate() {
            let color = SERIES[k];
            if s.dots {
                for (i, &v) in s.values.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                    draw_circle(f.x(i), f.y(v), 4.0, color);
                }
                continue;
            }
            let points: Vec<(usize, f64)> = s.values.iter().copied().enumerate().filter(|(_, v)| v.is_finite()).collect();
            for pair in points.windows(2) {
                let ((i0, v0), (i1, v1)) = (pair[0], pair[1]);
                draw_line(f.x(i0), f.y(v0), f.x(i1), f.y(v1), 2.0, color);
            }
        }
    }

    fn draw_stacked(&self, f: &Frame) {
        let count = f.count;
        let mut base = vec![0.0; count];
        for (k, s) in self.series.iter().enumerate() {
            let top: Vec<f64> = (0..count).map(|i| base[i] + s.values[i].max(0.0) * 100.0).collect();
            for i in 1..count {
                let (x0, x1) = (f.x(i - 1), f.x(i));
                let (b0, b1, t0, t1) = (f.y(base[i - 1]), f.y(base[i]), f.y(top[i - 1]), f.y(top[i]));
                draw_triangle(vec2(x0, b0), vec2(x1, b1), vec2(x1, t1), SERIES[k]);
                draw_triangle(vec2(x0, b0), vec2(x1, t1), vec2(x0, t0), SERIES[k]);
            }
            // Surface gap between bands.
            for i in 1..count {
                draw_line(f.x(i - 1), f.y(top[i - 1]), f.x(i), f.y(top[i]), 2.0, theme::PANEL_SOLID);
            }
            base = top;
        }
    }

    /// Tooltip lines for one generation.
    pub fn values_at(&self, row: usize) -> Vec<(usize, String)> {
        self.series
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                let v = *s.values.get(row)?;
                if !v.is_finite() {
                    return None;
                }
                let shown = match self.kind {
                    Kind::Stacked => format!("{:.0}%", v * 100.0),
                    Kind::Lines if self.unit == "%" => format!("{v:.0}%"),
                    Kind::Lines if v.abs() >= 100.0 => format!("{v:.0}"),
                    Kind::Lines => format!("{v:.2}"),
                };
                Some((k, format!("{}  {shown}", s.name)))
            })
            .collect()
    }
}

/// A tooltip box next to the mouse.
pub fn tooltip(heading: &str, lines: &[(usize, String)]) {
    let (mx, my) = mouse_position();
    let width = lines
        .iter()
        .map(|(_, l)| measure_text(l, None, 14, 1.0).width + 20.0)
        .chain(std::iter::once(measure_text(heading, None, 14, 1.0).width))
        .fold(0.0, f32::max)
        + 20.0;
    let height = 26.0 + lines.len() as f32 * 18.0;
    let x = if mx + 16.0 + width > screen_width() { mx - width - 16.0 } else { mx + 16.0 };
    let y = (my + 12.0).min(screen_height() - height - 4.0);
    draw_rectangle(x, y, width, height, theme::PANEL_SOLID);
    draw_rectangle_lines(x, y, width, height, 1.0, theme::TEXT_DIM);
    text(heading, x + 10.0, y + 18.0, 14.0, theme::TEXT);
    for (i, (k, line)) in lines.iter().enumerate() {
        let ly = y + 36.0 + i as f32 * 18.0;
        draw_rectangle(x + 10.0, ly - 10.0, 10.0, 10.0, SERIES[*k]);
        text(line, x + 26.0, ly, 14.0, theme::TEXT_DIM);
    }
}
