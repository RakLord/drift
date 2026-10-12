//! Uniform grid over boundary segments, for fast neighbourhood queries and
//! raycasts.

use common::Vec2;

use super::boundary::{Segment, SegmentKind};

const CELL: f64 = 8.0;

pub struct SegmentGrid {
    origin: Vec2,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub distance: f64,
    pub kind: SegmentKind,
}

impl SegmentGrid {
    pub fn build(segments: &[Segment]) -> Self {
        let mut min = Vec2::new(f64::MAX, f64::MAX);
        let mut max = Vec2::new(f64::MIN, f64::MIN);
        for s in segments {
            for p in [s.a, s.b] {
                min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
                max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
            }
        }
        let origin = min - Vec2::new(CELL, CELL);
        let columns = ((max.x - origin.x) / CELL).ceil() as usize + 2;
        let rows = ((max.y - origin.y) / CELL).ceil() as usize + 2;
        let mut grid = Self { origin, columns, rows, cells: vec![vec![]; columns * rows] };
        for (index, s) in segments.iter().enumerate() {
            let lo = Vec2::new(s.a.x.min(s.b.x), s.a.y.min(s.b.y));
            let hi = Vec2::new(s.a.x.max(s.b.x), s.a.y.max(s.b.y));
            grid.for_cells(lo, hi, |cell| grid_push(cell, index));
        }
        grid
    }

    fn cell_coords(&self, p: Vec2) -> (i64, i64) {
        (((p.x - self.origin.x) / CELL).floor() as i64, ((p.y - self.origin.y) / CELL).floor() as i64)
    }

    fn cell_index(&self, cx: i64, cy: i64) -> Option<usize> {
        let in_range = cx >= 0 && cy >= 0 && (cx as usize) < self.columns && (cy as usize) < self.rows;
        in_range.then(|| cy as usize * self.columns + cx as usize)
    }

    fn for_cells(&mut self, lo: Vec2, hi: Vec2, mut f: impl FnMut(&mut Vec<u32>)) {
        let (x0, y0) = self.cell_coords(lo);
        let (x1, y1) = self.cell_coords(hi);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if let Some(i) = self.cell_index(cx, cy) {
                    f(&mut self.cells[i]);
                }
            }
        }
    }

    /// Indices of segments that may overlap the box `lo..hi`, sorted and
    /// deduplicated (so results are deterministic).
    pub fn query(&self, lo: Vec2, hi: Vec2, out: &mut Vec<u32>) {
        out.clear();
        let (x0, y0) = self.cell_coords(lo);
        let (x1, y1) = self.cell_coords(hi);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if let Some(i) = self.cell_index(cx, cy) {
                    out.extend_from_slice(&self.cells[i]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Nearest segment hit along a ray, walking the grid cell by cell.
    /// `direction` must be a unit vector. `accept` filters segment kinds.
    pub fn raycast(
        &self,
        segments: &[Segment],
        origin: Vec2,
        direction: Vec2,
        max_distance: f64,
        accept: impl Fn(SegmentKind) -> bool,
    ) -> Option<RayHit> {
        let (mut cx, mut cy) = self.cell_coords(origin);
        let step_x: i64 = if direction.x >= 0.0 { 1 } else { -1 };
        let step_y: i64 = if direction.y >= 0.0 { 1 } else { -1 };
        let next_boundary = |c: i64, step: i64, o: f64, start: f64| o + (c + i64::from(step > 0)) as f64 * CELL - start;
        let mut t_max_x = if direction.x != 0.0 {
            next_boundary(cx, step_x, self.origin.x, origin.x) / direction.x
        } else {
            f64::INFINITY
        };
        let mut t_max_y = if direction.y != 0.0 {
            next_boundary(cy, step_y, self.origin.y, origin.y) / direction.y
        } else {
            f64::INFINITY
        };
        let t_delta_x = if direction.x != 0.0 { CELL / direction.x.abs() } else { f64::INFINITY };
        let t_delta_y = if direction.y != 0.0 { CELL / direction.y.abs() } else { f64::INFINITY };

        let mut best: Option<RayHit> = None;
        let mut travelled = 0.0;
        while travelled <= max_distance {
            if let Some(i) = self.cell_index(cx, cy) {
                for &index in &self.cells[i] {
                    let s = &segments[index as usize];
                    if !accept(s.kind) {
                        continue;
                    }
                    if let Some(t) = ray_segment(origin, direction, s)
                        && t <= max_distance
                        && best.is_none_or(|b| t < b.distance)
                    {
                        best = Some(RayHit { distance: t, kind: s.kind });
                    }
                }
            }
            // Anything found closer than the next cell boundary is final.
            let exit = t_max_x.min(t_max_y);
            if best.is_some_and(|b| b.distance <= exit) {
                break;
            }
            if t_max_x < t_max_y {
                cx += step_x;
                travelled = t_max_x;
                t_max_x += t_delta_x;
            } else {
                cy += step_y;
                travelled = t_max_y;
                t_max_y += t_delta_y;
            }
        }
        best
    }
}

fn grid_push(cell: &mut Vec<u32>, index: usize) {
    cell.push(index as u32);
}

/// Distance along the ray to the segment, if it is hit.
fn ray_segment(origin: Vec2, direction: Vec2, s: &Segment) -> Option<f64> {
    let e = s.b - s.a;
    let denom = direction.cross(e);
    if denom.abs() < 1e-12 {
        return None;
    }
    let diff = s.a - origin;
    let t = diff.cross(e) / denom;
    let u = diff.cross(direction) / denom;
    (t >= 0.0 && (0.0..=1.0).contains(&u)).then_some(t)
}
