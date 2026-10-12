//! Track rendering: grass verges, asphalt, painted edges, concrete walls,
//! verge fences and the start line. Geometry is built once per track.

use common::Vec2;
use macroquad::prelude::*;
use sim::track::{EdgeKind, Side, Track, ZoneKind};

use super::camera::FollowCamera;
use super::{draw_box, draw_segment, theme, to_mq};

const WALL_THICKNESS: f64 = 0.7;
const WALL_BLOCK: usize = 4; // samples per alternating barrier block
const PAINT_WIDTH: f64 = 0.18;
const PAINT_INSET: f64 = 0.3;
const FENCE_POST_EVERY: usize = 5;
const ZONE_WIDTH: f64 = 1.6;
const ZONE_STRIPE: usize = 2; // samples per stripe
const CLIP_RADIUS: f64 = 0.6;

/// Per-sample cross-section points, from the centreline outwards.
struct Slice {
    centre: Vec2,
    normal: Vec2,
    /// Asphalt edge, left then right.
    edge: [Vec2; 2],
    /// Back of the verge (fence line), left then right.
    fence: [Vec2; 2],
    kind: [EdgeKind; 2],
    /// Whether the edge / fence from this slice to the next exists (they're
    /// left out across crossroads).
    edge_present: [bool; 2],
    fence_present: [bool; 2],
}

/// A judged zone, ready to draw.
struct ZoneMesh {
    kind: ZoneKind,
    /// 0 = left, 1 = right.
    side: usize,
    /// Outer zones: the samples it covers, in driving order.
    samples: Vec<usize>,
    /// Clips: the clipping point.
    point: Vec2,
}

pub struct TrackMesh {
    slices: Vec<Slice>,
    zones: Vec<ZoneMesh>,
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
                    edge_present: SIDES.map(|side| track.edge_present(i, side)),
                    fence_present: SIDES.map(|side| track.fence_present(i, side)),
                }
            })
            .collect();
        let zones = track
            .zones
            .iter()
            .map(|z| {
                let n = track.samples.len();
                let first = track.samples.iter().position(|s| s.distance >= z.start).unwrap_or(0);
                let samples = match z.kind {
                    ZoneKind::Outer => (0..n)
                        .map(|k| (first + k) % n)
                        .take_while(|&i| track.in_range(track.samples[i].distance, z.start, z.end))
                        .collect(),
                    ZoneKind::Clip => vec![],
                };
                let side = if z.side == Side::Left { 0 } else { 1 };
                ZoneMesh { kind: z.kind, side, samples, point: z.point }
            })
            .collect();
        Self { slices, zones, half_width: half, verge }
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
                        if a.edge_present[side] {
                            draw_segment(a.edge[side] + inset, b.edge[side] + inset_b, PAINT_WIDTH, theme::EDGE_PAINT);
                        }
                        if a.fence_present[side] {
                            draw_segment(a.fence[side], b.fence[side], 0.12, theme::FENCE);
                            if i % FENCE_POST_EVERY == 0 {
                                let p = to_mq(a.fence[side]);
                                draw_circle(p.x, p.y, 0.15, theme::FENCE);
                            }
                        }
                    }
                    EdgeKind::Hard if !a.edge_present[side] => {}
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

    /// Zones painted on the asphalt; `lit[i]` brightens zone `i` while it is
    /// being scored.
    pub fn draw_zones(&self, lit: &[bool]) {
        for (zone, &lit) in self.zones.iter().zip(lit.iter().chain(std::iter::repeat(&false))) {
            match zone.kind {
                ZoneKind::Outer => {
                    let sign = if zone.side == 0 { 1.0 } else { -1.0 };
                    let (bright, dim) = if lit { (0.85, 0.45) } else { (0.35, 0.12) };
                    for (k, pair) in zone.samples.windows(2).enumerate() {
                        let (a, b) = (&self.slices[pair[0]], &self.slices[pair[1]]);
                        let inner_a = a.edge[zone.side] - a.normal * (sign * ZONE_WIDTH);
                        let inner_b = b.edge[zone.side] - b.normal * (sign * ZONE_WIDTH);
                        let alpha = if (k / ZONE_STRIPE).is_multiple_of(2) { bright } else { dim };
                        let color = Color { a: alpha, ..theme::NEON_PINK };
                        quad(a.edge[zone.side], b.edge[zone.side], inner_b, inner_a, color);
                    }
                }
                ZoneKind::Clip => {
                    let p = to_mq(zone.point);
                    if lit {
                        draw_circle(p.x, p.y, (CLIP_RADIUS * 2.5) as f32, Color { a: 0.25, ..theme::SODIUM });
                    }
                    draw_circle(p.x, p.y, CLIP_RADIUS as f32, theme::SODIUM);
                    draw_circle(p.x, p.y, (CLIP_RADIUS * 0.45) as f32, theme::EDGE_PAINT);
                }
            }
        }
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
