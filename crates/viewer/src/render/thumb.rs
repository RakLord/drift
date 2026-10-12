//! A whole track drawn small into a screen rectangle: road, hard walls,
//! start line and direction. Screen space, no camera, so it fits anywhere
//! (cards, detail panes).

use common::Vec2;
use macroquad::prelude::*;
use sim::track::{EdgeKind, Side, Track};

use super::{text, theme};

/// Keep about one centreline point per this many metres.
const STEP_M: f64 = 4.0;

pub struct Thumb {
    /// Decimated centreline, closed (the first point isn't repeated).
    centre: Vec<Vec2>,
    /// Hard-wall runs, each a polyline along the track edge.
    walls: Vec<Vec<Vec2>>,
    lo: Vec2,
    hi: Vec2,
    half_width: f64,
    start: Vec2,
    /// Driving direction at the start, unit.
    heading: Vec2,
    /// Apex of each corner, with the direction to the outside of the turn.
    apexes: Vec<(Vec2, Vec2)>,
    crossings: Vec<Vec2>,
}

#[derive(Clone, Copy)]
pub struct ThumbStyle {
    pub road: Color,
    pub wall: Color,
    pub start: Color,
    /// Number the corners and mark crossings.
    pub details: bool,
}

impl ThumbStyle {
    pub const CARD: Self = Self { road: Color::new(0.42, 0.44, 0.52, 1.0), wall: theme::SODIUM, start: theme::NEON_CYAN, details: false };
    /// Off / not trained on: the same map, faded.
    pub const DIM: Self = Self {
        road: Color::new(0.26, 0.27, 0.32, 1.0),
        wall: Color::new(0.45, 0.35, 0.22, 1.0),
        start: Color::new(0.1, 0.45, 0.52, 1.0),
        details: false,
    };
    pub const DETAIL: Self = Self { details: true, ..Self::CARD };
}

impl Thumb {
    pub fn new(track: &Track) -> Self {
        let n = track.samples.len();
        let every = ((STEP_M / (track.length / n as f64)).round() as usize).max(1);
        let half_width = track.half_width();
        let centre: Vec<Vec2> = track.samples.iter().step_by(every).map(|s| s.position).collect();

        let mut walls = vec![];
        for side in [Side::Left, Side::Right] {
            let edge_at = |i: usize| {
                let s = &track.samples[i];
                s.position + s.normal() * (side.sign() * half_width)
            };
            let hard = |i: usize| track.edge(i, side) == EdgeKind::Hard && track.edge_present(i, side);
            // Start a run where a wall begins, so a run wrapping past the
            // start line stays in one piece.
            let first = (0..n).find(|&i| !hard(i)).unwrap_or(0);
            let mut run: Vec<Vec2> = vec![];
            for k in 0..=n {
                let i = (first + k) % n;
                if hard(i) {
                    if run.is_empty() || k % every == 0 {
                        run.push(edge_at(i));
                    }
                } else if !run.is_empty() {
                    run.push(edge_at(i));
                    walls.push(std::mem::take(&mut run));
                }
            }
            if run.len() > 1 {
                walls.push(run);
            }
        }

        let (lo, hi) = track.samples.iter().fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(lo, hi), s| {
            let p = s.position;
            (Vec2::new(lo.x.min(p.x), lo.y.min(p.y)), Vec2::new(hi.x.max(p.x), hi.y.max(p.y)))
        });
        let pad = Vec2::new(1.0, 1.0) * half_width;
        let first = &track.samples[0];
        Self {
            centre,
            walls,
            lo: lo - pad,
            hi: hi + pad,
            half_width,
            start: first.position,
            heading: first.tangent,
            apexes: track
                .corners
                .iter()
                .map(|c| {
                    let s = &track.samples[c.apex];
                    (s.position, s.normal() * -c.direction)
                })
                .collect(),
            crossings: track.crossings.iter().map(|c| c.point).collect(),
        }
    }

    /// Draw the whole track centred in `rect`, keeping its proportions.
    pub fn draw(&self, rect: Rect, style: ThumbStyle) {
        let size = self.hi - self.lo;
        let scale = (f64::from(rect.w) / size.x).min(f64::from(rect.h) / size.y);
        let (ox, oy) = ((f64::from(rect.w) - size.x * scale) / 2.0, (f64::from(rect.h) - size.y * scale) / 2.0);
        // World +y is up, screen +y is down.
        let at = |p: Vec2| {
            vec2(rect.x + (ox + (p.x - self.lo.x) * scale) as f32, rect.y + (oy + (self.hi.y - p.y) * scale) as f32)
        };

        let road = ((2.0 * self.half_width * scale) as f32).max(3.0);
        let points: Vec<macroquad::math::Vec2> = self.centre.iter().map(|&p| at(p)).collect();
        for (i, &a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            draw_line(a.x, a.y, b.x, b.y, road, style.road);
            // Round joints, so bends don't show notches.
            if road > 4.0 {
                draw_circle(a.x, a.y, road / 2.0, style.road);
            }
        }
        let wall = (road * 0.2).clamp(1.0, 3.0);
        for run in &self.walls {
            for pair in run.windows(2) {
                let (a, b) = (at(pair[0]), at(pair[1]));
                draw_line(a.x, a.y, b.x, b.y, wall, style.wall);
            }
        }

        // Start line across the road, with a tick pointing the way to drive.
        // Sized in pixels so a wide skidpad doesn't get a giant arrow.
        let base = at(self.start);
        let forward = vec2(self.heading.x as f32, -self.heading.y as f32);
        let across = forward.perp() * (road / 2.0 + 3.0);
        draw_line(base.x + across.x, base.y + across.y, base.x - across.x, base.y - across.y, 2.5, style.start);
        let size = if style.details { 16.0 } else { 11.0 };
        let tip = base + forward * size;
        let side = forward.perp() * (size * 0.45);
        draw_triangle(tip, base + side, base - side, style.start);

        if style.details {
            for p in &self.crossings {
                let c = at(*p);
                draw_circle_lines(c.x, c.y, road * 0.9, 1.5, theme::NEON_PINK);
            }
            for (i, (apex, outside)) in self.apexes.iter().enumerate() {
                let p = at(*apex + *outside * (self.half_width + 12.0 / scale));
                let label = (i + 1).to_string();
                let w = measure_text(&label, None, 16, 1.0).width;
                text(&label, p.x - w / 2.0, p.y + 5.0, 16.0, theme::TEXT);
            }
        }
    }
}
