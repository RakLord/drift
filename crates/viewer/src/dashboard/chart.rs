//! Small multiples for run metrics: one y-axis per chart, a recessive grid,
//! a legend carrying each series' value at the active generation, bold
//! trend lines over faint raw values, stage changes marked, and shared
//! hover and pinned crosshairs.

use std::ops::Range;

use macroquad::prelude::*;

use super::trend::rolling_mean;
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

/// Series `k`'s colour (repeating past six).
pub fn colour(k: usize) -> Color {
    SERIES[k % SERIES.len()]
}

pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    /// Draw as dots (sparse values, e.g. test results) instead of a line.
    pub dots: bool,
}

impl Series {
    pub fn line(name: impl Into<String>, values: Vec<f64>) -> Self {
        Self { name: name.into(), values, dots: false }
    }

    pub fn dots(name: impl Into<String>, values: Vec<f64>) -> Self {
        Self { name: name.into(), values, dots: true }
    }

    /// Logged at all in this run (series that never were are left out).
    fn logged(&self) -> bool {
        self.values.iter().any(|v| v.is_finite())
    }

    /// The value at `row`, or the last one logged before it.
    fn at_or_before(&self, row: usize) -> Option<f64> {
        let end = (row + 1).min(self.values.len());
        self.values[..end].iter().rev().copied().find(|v| v.is_finite())
    }
}

pub enum Kind {
    Lines,
    /// Shares stacked to 100%.
    Stacked,
    /// The first two series are the low and high edges of a shaded band
    /// (e.g. worst..best); all are also drawn as lines.
    Band,
}

/// How lines are drawn when there are enough generations for a trend.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// Bold trend over faint raw values.
    Both,
    Trend,
    Raw,
}

impl LineStyle {
    pub fn next(self) -> Self {
        match self {
            Self::Both => Self::Trend,
            Self::Trend => Self::Raw,
            Self::Raw => Self::Both,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Both => "trend + raw",
            Self::Trend => "trend only",
            Self::Raw => "raw only",
        }
    }
}

pub struct Chart {
    pub title: &'static str,
    pub unit: &'static str,
    pub series: Vec<Series>,
    pub kind: Kind,
    /// Fixed y-range, or None to fit the data.
    pub range: Option<(f64, f64)>,
}

/// What every chart on a page shares this frame.
pub struct View<'a> {
    /// Rows on screen (the zoom window).
    pub rows: Range<usize>,
    /// Rows per trend window; None draws raw values only.
    pub trend: Option<usize>,
    pub style: LineStyle,
    pub stages: &'a [(usize, String)],
    pub generations: &'a [f64],
    pub hover: Option<usize>,
    pub pinned: Option<usize>,
}

impl View<'_> {
    fn shows_raw(&self) -> bool {
        self.trend.is_none() || self.style != LineStyle::Trend
    }

    fn shows_trend(&self) -> bool {
        self.trend.is_some() && self.style != LineStyle::Raw
    }

    /// The row the legends read: pinned, else hovered, else the latest.
    fn active(&self) -> usize {
        self.pinned.or(self.hover).unwrap_or(self.rows.end.saturating_sub(1))
    }
}

pub struct Frame {
    rect: Rect,
    plot: Rect,
    lo: f64,
    hi: f64,
    rows: Range<usize>,
}

const PAD_LEFT: f32 = 52.0;
/// Below the title; legend rows add to this.
const PAD_TOP: f32 = 40.0;
const LEGEND_ROW: f32 = 22.0;
const PAD_BOTTOM: f32 = 24.0;
const PAD_RIGHT: f32 = 14.0;

impl Frame {
    fn x(&self, row: usize) -> f32 {
        let span = (self.rows.len().max(2) - 1) as f32;
        self.plot.x + self.plot.w * row.saturating_sub(self.rows.start) as f32 / span
    }

    fn y(&self, v: f64) -> f32 {
        let t = ((v - self.lo) / (self.hi - self.lo).max(1e-9)) as f32;
        self.plot.y + self.plot.h * (1.0 - t.clamp(0.0, 1.0))
    }

    /// Row under the mouse, if it's over this chart's plot.
    pub fn hovered(&self) -> Option<usize> {
        let (mx, my) = mouse_position();
        let inside = mx >= self.plot.x && mx <= self.plot.right() && my >= self.rect.y && my <= self.rect.bottom();
        if !inside || self.rows.is_empty() {
            return None;
        }
        let t = (mx - self.plot.x) / self.plot.w;
        let offset = (t * (self.rows.len().max(2) - 1) as f32).round() as usize;
        Some(self.rows.start + offset.min(self.rows.len() - 1))
    }

    /// Rows on screen, thinned to about one per pixel column.
    fn sampled(&self) -> Vec<usize> {
        let step = (self.rows.len() as f32 / self.plot.w.max(1.0)).ceil().max(1.0) as usize;
        let mut rows: Vec<usize> = self.rows.clone().step_by(step).collect();
        if let Some(last) = self.rows.clone().last()
            && rows.last() != Some(&last)
        {
            rows.push(last);
        }
        rows
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

impl Chart {
    /// Each series' trend (None for dots, or when the view has no trend).
    fn trends(&self, view: &View) -> Vec<Option<Vec<f64>>> {
        self.series
            .iter()
            .map(|s| match view.trend {
                Some(w) if !s.dots => Some(rolling_mean(&s.values, w)),
                _ => None,
            })
            .collect()
    }

    /// The values a series is drawn from: its trend when shown, else raw.
    fn drawn<'a>(&'a self, k: usize, view: &View, trends: &'a [Option<Vec<f64>>]) -> &'a [f64] {
        match &trends[k] {
            Some(t) if view.shows_trend() => t,
            _ => &self.series[k].values,
        }
    }

    fn fit(&self, view: &View, trends: &[Option<Vec<f64>>]) -> (f64, f64) {
        if let Some(r) = self.range {
            return r;
        }
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for (s, t) in self.series.iter().zip(trends) {
            let raw = (view.shows_raw() || t.is_none()).then_some(&s.values[..]);
            let trend = t.as_deref().filter(|_| view.shows_trend());
            for values in [raw, trend].into_iter().flatten() {
                for &v in values.get(view.rows.clone()).unwrap_or(&[]).iter().filter(|v| v.is_finite()) {
                    (lo, hi) = (lo.min(v), hi.max(v));
                }
            }
        }
        if lo > hi {
            return (0.0, 1.0);
        }
        // The whole run starts at 0; a zoomed window fits its data tight.
        let lo = if view.rows.start == 0 { lo.min(0.0) } else { lo };
        let hi = if hi - lo < 1e-9 { lo + 1.0 } else { hi };
        let step = tick_step(hi - lo);
        ((lo / step).floor() * step, (hi / step).ceil() * step)
    }

    /// A value as the legend and tooltip show it.
    fn format(&self, v: f64) -> String {
        match self.kind {
            Kind::Stacked => format!("{:.0}%", v * 100.0),
            _ if self.unit == "%" => format!("{v:.0}%"),
            _ if v.abs() >= 100.0 => format!("{v:.0}{}", self.unit),
            _ => format!("{v:.2}{}", self.unit),
        }
    }

    pub fn draw(&self, rect: Rect, view: &View) -> Frame {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, theme::PANEL_SOLID);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, theme::GRID_MAJOR);
        text(self.title, rect.x + 12.0, rect.y + 22.0, 20.0, theme::TEXT);

        let shown: Vec<usize> = (0..self.series.len()).filter(|&k| self.series[k].logged()).collect();
        if shown.is_empty() {
            let message = "not logged in this run";
            let w = measure_text(message, None, 16, 1.0).width;
            text(message, rect.x + (rect.w - w) / 2.0, rect.y + rect.h / 2.0, 16.0, theme::TEXT_DIM);
            return Frame { rect, plot: Rect::new(0.0, 0.0, 0.0, 0.0), lo: 0.0, hi: 1.0, rows: 0..0 };
        }
        let trends = self.trends(view);
        let (lo, hi) = self.fit(view, &trends);

        // Each series' value at the active row. One series: right of the
        // title. Two or more: a legend on its own rows under the title,
        // wrapping as needed (coloured swatch, then name and value).
        let active = view.active();
        let value = |k: usize| self.series[k].at_or_before(active).map(|v| self.format(v));
        let mut legend_rows = 0.0;
        if let [k] = shown[..] {
            if let Some(v) = value(k) {
                let w = measure_text(&v, None, 18, 1.0).width;
                text(&v, rect.right() - 12.0 - w, rect.y + 22.0, 18.0, theme::TEXT_DIM);
            }
        } else {
            legend_rows = 1.0;
            let (left, right) = (rect.x + 12.0, rect.right() - 12.0);
            let mut x = left;
            for &k in &shown {
                let name = &self.series[k].name;
                let label = value(k).map_or_else(|| name.clone(), |v| format!("{name}  {v}"));
                let w = 16.0 + measure_text(&label, None, 17, 1.0).width;
                if x + w > right && x > left {
                    x = left;
                    legend_rows += 1.0;
                }
                let y = rect.y + 22.0 + legend_rows * LEGEND_ROW;
                draw_rectangle(x, y - 12.0, 12.0, 12.0, colour(k));
                text(&label, x + 16.0, y, 17.0, theme::TEXT_DIM);
                x += w + 14.0;
            }
        }
        let top = PAD_TOP + legend_rows * LEGEND_ROW;
        let plot = Rect::new(rect.x + PAD_LEFT, rect.y + top, rect.w - PAD_LEFT - PAD_RIGHT, rect.h - top - PAD_BOTTOM);
        let frame = Frame { rect, plot, lo, hi, rows: view.rows.clone() };

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
        self.draw_x_ticks(&frame, view);

        // Stage changes.
        for (row, name) in view.stages.iter().filter(|(row, _)| *row > view.rows.start && *row < view.rows.end) {
            let x = frame.x(*row);
            let mut y = plot.y;
            while y < plot.bottom() {
                draw_line(x, y, x, (y + 4.0).min(plot.bottom()), 1.0, theme::TEXT_DIM);
                y += 8.0;
            }
            text(name, x + 4.0, plot.y + 12.0, 13.0, theme::TEXT_DIM);
        }

        match self.kind {
            Kind::Lines => self.draw_lines(&frame, view, &trends, &shown),
            Kind::Stacked => self.draw_stacked(&frame, view, &trends),
            Kind::Band => {
                self.draw_band(&frame, view, &trends);
                self.draw_lines(&frame, view, &trends, &shown);
            }
        }

        if let Some(row) = view.pinned.filter(|r| view.rows.contains(r)) {
            let x = frame.x(row);
            draw_line(x, plot.y, x, plot.bottom(), 2.0, Color { a: 0.7, ..theme::NEON_CYAN });
        }
        if let Some(row) = view.hover.filter(|r| view.rows.contains(r)) {
            let x = frame.x(row);
            draw_line(x, plot.y, x, plot.bottom(), 1.0, theme::TEXT_DIM);
        }
        // Dots where the bold lines cross the hovered (or pinned) row.
        if let Some(row) = view.hover.or(view.pinned).filter(|r| view.rows.contains(r))
            && !matches!(self.kind, Kind::Stacked)
        {
            for &k in &shown {
                if let Some(&v) = self.drawn(k, view, &trends).get(row).filter(|v| v.is_finite()) {
                    draw_circle(frame.x(row), frame.y(v), 5.0, theme::PANEL_SOLID);
                    draw_circle(frame.x(row), frame.y(v), 4.0, colour(k));
                }
            }
        }
        frame
    }

    /// Generation labels along the bottom at "nice" steps, and "gen".
    fn draw_x_ticks(&self, f: &Frame, view: &View) {
        let y = f.rect.bottom() - 7.0;
        text("gen", f.rect.x + 12.0, y, 13.0, theme::TEXT_DIM);
        let generations = view.generations.get(view.rows.clone()).unwrap_or(&[]);
        let (Some(&first), Some(&last)) = (generations.first(), generations.last()) else { return };
        if !first.is_finite() || !last.is_finite() {
            return;
        }
        let step = tick_step(last - first).max(1.0);
        for (i, &g) in generations.iter().enumerate() {
            if !g.is_finite() || g.rem_euclid(step) > 1e-6 {
                continue;
            }
            let x = f.x(view.rows.start + i);
            draw_line(x, f.plot.bottom(), x, f.plot.bottom() + 4.0, 1.0, theme::GRID_MAJOR);
            let label = format!("{g:.0}");
            let w = measure_text(&label, None, 13, 1.0).width;
            text(&label, (x - w / 2.0).clamp(f.plot.x, f.plot.right() - w), y, 13.0, theme::TEXT_DIM);
        }
    }

    fn draw_lines(&self, f: &Frame, view: &View, trends: &[Option<Vec<f64>>], shown: &[usize]) {
        for &k in shown {
            let (s, color) = (&self.series[k], colour(k));
            if s.dots {
                for row in f.rows.clone() {
                    if let Some(&v) = s.values.get(row).filter(|v| v.is_finite()) {
                        draw_circle(f.x(row), f.y(v), 4.0, color);
                    }
                }
                continue;
            }
            let trend = trends[k].as_deref().filter(|_| view.shows_trend());
            if view.shows_raw() || trend.is_none() {
                match trend {
                    Some(_) => polyline(f, &s.values, 1.0, Color { a: 0.35, ..color }),
                    None => polyline(f, &s.values, 2.0, color),
                }
            }
            if let Some(t) = trend {
                polyline(f, t, 2.0, color);
            }
        }
    }

    fn draw_stacked(&self, f: &Frame, view: &View, trends: &[Option<Vec<f64>>]) {
        let rows = f.sampled();
        let mut base = vec![0.0; rows.len()];
        for k in 0..self.series.len() {
            let values = self.drawn(k, view, trends);
            // NaN.max(0.0) is 0: unlogged shares stack as nothing.
            let top: Vec<f64> = rows.iter().zip(&base).map(|(&r, b)| b + values[r].max(0.0) * 100.0).collect();
            for j in 1..rows.len() {
                let (x0, x1) = (f.x(rows[j - 1]), f.x(rows[j]));
                let (b0, b1, t0, t1) = (f.y(base[j - 1]), f.y(base[j]), f.y(top[j - 1]), f.y(top[j]));
                draw_triangle(vec2(x0, b0), vec2(x1, b1), vec2(x1, t1), colour(k));
                draw_triangle(vec2(x0, b0), vec2(x1, t1), vec2(x0, t0), colour(k));
            }
            // Surface gap between bands.
            for j in 1..rows.len() {
                draw_line(f.x(rows[j - 1]), f.y(top[j - 1]), f.x(rows[j]), f.y(top[j]), 2.0, theme::PANEL_SOLID);
            }
            base = top;
        }
    }

    fn draw_band(&self, f: &Frame, view: &View, trends: &[Option<Vec<f64>>]) {
        if self.series.len() < 2 {
            return;
        }
        let (low, high) = (self.drawn(0, view, trends), self.drawn(1, view, trends));
        let fill = Color { a: 0.18, ..colour(0) };
        for pair in f.sampled().windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if ![low[a], low[b], high[a], high[b]].iter().all(|v| v.is_finite()) {
                continue;
            }
            let (x0, x1) = (f.x(a), f.x(b));
            let (l0, l1, h0, h1) = (f.y(low[a]), f.y(low[b]), f.y(high[a]), f.y(high[b]));
            draw_triangle(vec2(x0, l0), vec2(x1, l1), vec2(x1, h1), fill);
            draw_triangle(vec2(x0, l0), vec2(x1, h1), vec2(x0, h0), fill);
        }
    }

    /// Tooltip lines for one generation: each series' value there, and its
    /// trend with the change over the last trend window.
    pub fn values_at(&self, row: usize, view: &View) -> Vec<(usize, String)> {
        let trends = self.trends(view);
        let finite = |v: &&f64| v.is_finite();
        self.series
            .iter()
            .enumerate()
            .filter_map(|(k, s)| {
                let v = *s.values.get(row).filter(finite)?;
                let mut line = format!("{}  {}", s.name, self.format(v));
                if let (Some(t), Some(w)) = (&trends[k], view.trend)
                    && let Some(&now) = t.get(row).filter(finite)
                {
                    line += &format!("    trend {}", self.format(now));
                    if let Some(&then) = row.checked_sub(w).and_then(|r| t.get(r)).filter(finite) {
                        let sign = if now >= then { "+" } else { "-" };
                        line += &format!(" ({sign}{})", self.format((now - then).abs()));
                    }
                }
                Some((k, line))
            })
            .collect()
    }
}

/// A `width` px line through the finite values on screen. With more
/// rows than pixels, each pixel column draws its min..max, so spikes stay.
fn polyline(f: &Frame, values: &[f64], width: f32, color: Color) {
    struct Column {
        px: i32,
        x: f32,
        first: f32,
        last: f32,
        top: f32,
        bottom: f32,
    }
    let mut previous: Option<Vec2> = None;
    let mut flush = |c: Column| {
        if let Some(p) = previous {
            draw_line(p.x, p.y, c.x, c.first, width, color);
        }
        if c.bottom - c.top > 0.5 {
            draw_line(c.x, c.top, c.x, c.bottom, width, color);
        }
        previous = Some(vec2(c.x, c.last));
    };
    let mut column: Option<Column> = None;
    for row in f.rows.clone() {
        let Some(&v) = values.get(row).filter(|v| v.is_finite()) else { continue };
        let (x, y) = (f.x(row), f.y(v));
        let px = x.floor() as i32;
        match &mut column {
            Some(c) if c.px == px => {
                c.last = y;
                c.top = c.top.min(y);
                c.bottom = c.bottom.max(y);
            }
            _ => {
                if let Some(c) = column.take() {
                    flush(c);
                }
                column = Some(Column { px, x, first: y, last: y, top: y, bottom: y });
            }
        }
    }
    if let Some(c) = column {
        flush(c);
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
        draw_rectangle(x + 10.0, ly - 10.0, 10.0, 10.0, colour(*k));
        text(line, x + 26.0, ly, 14.0, theme::TEXT_DIM);
    }
}
