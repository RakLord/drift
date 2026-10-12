//! Track rendering: grass verges, asphalt, painted edges, concrete walls,
//! verge fences and the start line. Geometry is built once per track.

use common::Vec2;
use macroquad::prelude::*;
use sim::track::{EdgeKind, Side, Track};

use super::camera::FollowCamera;
use super::{draw_box, draw_segment, theme, to_mq};

const WALL_THICKNESS: f64 = 0.7;
const WALL_BLOCK: usize = 4; // samples per alternating barrier block
const PAINT_WIDTH: f64 = 0.18;
const PAINT_INSET: f64 = 0.3;
const FENCE_POST_EVERY: usize = 5;

/// Per-sample cross-section points, from the centreline outwards.
struct Slice {
    centre: Vec2,
    normal: Vec2,
    /// Asphalt edge, left then right.
    edge: [Vec2; 2],
    /// Back of the verge (fence line), left then right.
    fence: [Vec2; 2],
    kind: [EdgeKind; 2],
}

pub struct TrackMesh {
    slices: Vec<Slice>,
    half_width: f64,
    verge: f64,
}

const SIDES: [Side; 2] = [Side::Left, Side::Right];

impl TrackMesh {
    pub fn new(track: &Track) -> Self {
        let half = track.half_width();
        let verge = track.def.verge;
        let slices = track
            .samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let n = s.normal();
                Slice {
                    centre: s.position,
                    normal: n,
                    edge: SIDES.map(|side| s.position + n * (side.sign() * half)),
                    fence: SIDES.map(|side| s.position + n * (side.sign() * (half + verge))),
                    kind: SIDES.map(|side| track.edge(i, side)),
                }
            })
            .collect();
        Self { slices, half_width: half, verge }
    }

    pub fn draw(&self, camera: &FollowCamera) {
        let (min, max) = camera.visible();
        let reach = self.half_width + self.verge + 2.0;
        let on_screen = |p: Vec2| {
            p.x > min.x - reach && p.x < max.x + reach && p.y > min.y - reach && p.y < max.y + reach
        };
        let n = self.slices.len();
        let visible: Vec<usize> = (0..n).filter(|&i| on_screen(self.slices[i].centre)).collect();
        let pairs = || visible.iter().map(move |&i| (i, &self.slices[i], &self.slices[(i + 1) % n]));

        // Ground layers first, then everything that sits on top.
        for (_, a, b) in pairs() {
            for side in 0..2 {
                if a.kind[side] == EdgeKind::Soft {
                    quad(a.edge[side], b.edge[side], b.fence[side], a.fence[side], theme::GRASS);
                }
            }
        }
        for (_, a, b) in pairs() {
            quad(a.edge[0], b.edge[0], b.edge[1], a.edge[1], theme::ASPHALT);
        }
        for (i, a, b) in pairs() {
            for (side, sign) in [(0, 1.0), (1, -1.0)] {
                let inset = a.normal * (-sign * PAINT_INSET);
                let inset_b = b.normal * (-sign * PAINT_INSET);
                match a.kind[side] {
                    EdgeKind::Soft => {
                        draw_segment(a.edge[side] + inset, b.edge[side] + inset_b, PAINT_WIDTH, theme::EDGE_PAINT);
                        draw_segment(a.fence[side], b.fence[side], 0.12, theme::FENCE);
                        if i % FENCE_POST_EVERY == 0 {
                            let p = to_mq(a.fence[side]);
                            draw_circle(p.x, p.y, 0.15, theme::FENCE);
                        }
                    }
                    EdgeKind::Hard => {
                        let out_a = a.edge[side] + a.normal * (sign * WALL_THICKNESS);
                        let out_b = b.edge[side] + b.normal * (sign * WALL_THICKNESS);
                        let color = if (i / WALL_BLOCK).is_multiple_of(2) { theme::CONCRETE } else { theme::CONCRETE_DARK };
                        quad(a.edge[side], b.edge[side], out_b, out_a, color);
                    }
                }
            }
        }
        self.draw_start_line();
    }

    fn draw_start_line(&self) {
        let s = &self.slices[0];
        let tangent = -s.normal.perp();
        let squares = 10;
        let size = self.half_width * 2.0 / f64::from(squares);
        for row in 0..2 {
            for k in 0..squares {
                let across = -self.half_width + size * (f64::from(k) + 0.5);
                let along = size * (f64::from(row) + 0.5);
                let color = if (k + row) % 2 == 0 { theme::EDGE_PAINT } else { theme::ASPHALT };
                draw_box(s.centre + s.normal * across + tangent * along, Vec2::new(size, size), tangent.angle(), color);
            }
        }
    }
}

fn quad(a: Vec2, b: Vec2, c: Vec2, d: Vec2, color: Color) {
    let [a, b, c, d] = [a, b, c, d].map(to_mq);
    draw_triangle(a, b, c, color);
    draw_triangle(a, c, d, color);
}
