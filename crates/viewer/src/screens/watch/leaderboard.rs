//! Leaderboard (live multi-car view): every car with its island colour.
//! Compact, it lists the live score and drift angle, best first. Expanded
//! (Tab), it's a wide table that also shows why each car was picked
//! (fitness, drift quality, age) and how far it has got; click a column
//! header to sort by it. Click a row to follow that car.

use macroquad::prelude::*;

use super::racer::Racer;
use crate::render::{text, theme};
use crate::ui::{Panel, fit, inside};

const COMPACT_W: f32 = 420.0;
const EXPANDED_W: f32 = 880.0;
const ROW_H: f32 = 26.0;
const SIZE: f32 = 17.0;

/// What the expanded table sorts by.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Points,
    Rank,
    Fitness,
    Quality,
    Age,
    Progress,
}

/// Expanded table columns: heading, width (0 = takes the rest), the sort
/// its heading picks, right-aligned.
const COLUMNS: [(&str, f32, Option<Sort>, bool); 8] = [
    ("", 34.0, None, false),
    ("car (#rank, network)", 0.0, Some(Sort::Rank), false),
    ("fitness", 84.0, Some(Sort::Fitness), true),
    ("quality", 74.0, Some(Sort::Quality), true),
    ("age", 56.0, Some(Sort::Age), true),
    ("pts", 80.0, Some(Sort::Points), true),
    ("progress", 84.0, Some(Sort::Progress), true),
    ("  status", 104.0, None, false),
];

pub struct Leaderboard {
    pub open: bool,
    /// The wide table (Tab).
    pub expanded: bool,
    sort: Sort,
    panel: Panel,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self { open: true, expanded: false, sort: Sort::Points, panel: Panel::new(16.0, 110.0, COMPACT_W) }
    }

    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Draw (if open) within `max_w` of the left edge; returns the racer
    /// clicked, if any. `generation`: the heat's, for ages.
    pub fn show(
        &mut self,
        racers: &[Racer],
        followed: usize,
        generation: u32,
        max_w: f32,
        colour: impl Fn(&Racer) -> Color,
    ) -> Option<usize> {
        if !self.open {
            return None;
        }
        self.panel.width = if self.expanded { EXPANDED_W.min(max_w - 32.0).max(COMPACT_W) } else { COMPACT_W };
        let sort = if self.expanded { self.sort } else { Sort::Points };
        let mut order: Vec<usize> = (0..racers.len()).collect();
        order.sort_by(|&a, &b| sort_key(&racers[b], sort, generation).total_cmp(&sort_key(&racers[a], sort, generation)));

        let driving = racers.iter().filter(|r| r.end.is_none()).count();
        let hint = if self.expanded { "Tab: less" } else { "Tab: details" };
        self.panel.begin("LEADERBOARD", screen_height() - 140.0);
        self.panel.label(&format!("{driving} of {} driving   click to follow   {hint}", racers.len()), theme::TEXT_DIM);
        let clicked = if self.expanded {
            self.table(racers, &order, followed, generation, &colour)
        } else {
            self.compact(racers, &order, followed, &colour)
        };
        self.panel.end();
        clicked
    }

    fn compact(&mut self, racers: &[Racer], order: &[usize], followed: usize, colour: &impl Fn(&Racer) -> Color) -> Option<usize> {
        let mut clicked = None;
        for (place, &i) in order.iter().enumerate() {
            let r = &racers[i];
            let detail = format!("{:>6.0} pts  {}", r.live_score(), status(r));
            let label = format!("{}. {}", place + 1, r.label);
            if self.panel.list_item_swatch(&label, &detail, i == followed, colour(r)) {
                clicked = Some(i);
            }
        }
        clicked
    }

    fn table(
        &mut self,
        racers: &[Racer],
        order: &[usize],
        followed: usize,
        generation: u32,
        colour: &impl Fn(&Racer) -> Color,
    ) -> Option<usize> {
        if let Some(rect) = self.panel.area(ROW_H) {
            let mut x = rect.x;
            for ((heading, _, sort, right), w) in COLUMNS.iter().zip(widths(rect.w)) {
                let hot = sort.is_some() && inside(x, rect.y, w, rect.h);
                let colour = if sort == &Some(self.sort) {
                    theme::NEON_CYAN
                } else if hot {
                    theme::TEXT
                } else {
                    theme::TEXT_DIM
                };
                cell(heading, x, rect.y, w, *right, 15.0, colour);
                if hot && is_mouse_button_pressed(MouseButton::Left) {
                    self.sort = sort.unwrap_or(self.sort);
                }
                x += w;
            }
            draw_rectangle(rect.x, rect.bottom() + 2.0, rect.w, 1.0, theme::GRID_MAJOR);
        }
        let mut clicked = None;
        for (place, &i) in order.iter().enumerate() {
            let Some(rect) = self.panel.area(ROW_H) else { continue };
            let r = &racers[i];
            if i == followed {
                draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color { a: 0.22, ..theme::NEON_PINK });
                draw_rectangle(rect.x, rect.y, 3.0, rect.h, theme::NEON_PINK);
            } else if inside(rect.x, rect.y, rect.w, rect.h) {
                draw_rectangle(rect.x, rect.y, rect.w, rect.h, theme::GRID_MAJOR);
            }
            if inside(rect.x, rect.y, rect.w, rect.h) && is_mouse_button_pressed(MouseButton::Left) {
                clicked = Some(i);
            }
            let e = &r.entry;
            let age = age(r, generation).map_or_else(|| "-".to_owned(), |a| a.to_string());
            let cells = [
                format!("{}.", place + 1),
                r.label.clone(),
                format!("{:.0}", e.fitness),
                format!("{:.2}", e.drift_quality),
                age,
                format!("{:.0}", r.live_score()),
                format!("{:.0} m", r.episode().result().progress),
                format!("  {}", status(r)),
            ];
            let mut x = rect.x;
            for (k, (((_, _, _, right), w), value)) in COLUMNS.iter().zip(widths(rect.w)).zip(&cells).enumerate() {
                let mut cx = x;
                if k == 1 {
                    draw_rectangle(cx + 4.0, rect.y + rect.h / 2.0 - 5.0, 10.0, 10.0, colour(r));
                    cx += 20.0;
                }
                let tone = if k == 1 { theme::TEXT } else { theme::TEXT_DIM };
                cell(value, cx, rect.y, w - (cx - x), *right, SIZE, tone);
                x += w;
            }
        }
        clicked
    }
}

/// Column widths for a table `total` wide (the flexible one takes the rest).
fn widths(total: f32) -> impl Iterator<Item = f32> {
    let fixed: f32 = COLUMNS.iter().map(|c| c.1).sum();
    COLUMNS.iter().map(move |c| if c.1 > 0.0 { c.1 } else { (total - fixed).max(60.0) })
}

/// Text in a cell `w` wide at `x`, cut to fit, left or right aligned.
fn cell(value: &str, x: f32, top: f32, w: f32, right: bool, size: f32, colour: Color) {
    let shown = fit(value, w - 8.0, size);
    let tx = if right { x + w - 8.0 - measure_text(&shown, None, size as u16, 1.0).width } else { x + 4.0 };
    text(&shown, tx, top + 18.0, size, colour);
}

/// Live drift angle while running, else how it ended.
fn status(r: &Racer) -> String {
    let state = &r.episode().world.car.state;
    let angle = if state.speed() > 3.0 { state.body_slip_angle().abs().to_degrees() } else { 0.0 };
    r.end.map_or_else(|| format!("{angle:>3.0}°"), |end| end.name().to_owned())
}

/// Generations since it was created; None if the file doesn't say (written
/// before lineage was).
pub fn age(r: &Racer, generation: u32) -> Option<u32> {
    r.entry.training.as_ref().map(|_| generation.saturating_sub(r.entry.born))
}

/// Bigger sorts first: rank flipped so #1 leads; oldest first by age.
fn sort_key(r: &Racer, sort: Sort, generation: u32) -> f64 {
    match sort {
        Sort::Points => r.live_score(),
        Sort::Rank => -f64::from(r.entry.rank),
        Sort::Fitness => r.entry.fitness,
        Sort::Quality => r.entry.drift_quality,
        Sort::Age => age(r, generation).map_or(f64::MIN, f64::from),
        Sort::Progress => r.episode().result().progress,
    }
}
