//! Skid marks: rubber laid down by sliding tyres, kept as a ring buffer of
//! short quads. The oldest marks are overwritten once the buffer is full.

use common::Vec2;
use macroquad::prelude::*;

use crate::render::camera::FollowCamera;
use crate::render::{theme, to_mq};

const MAX_SEGMENTS: usize = 40_000;
const MIN_INTENSITY: f64 = 0.04;
/// Lay a new segment once the wheel has moved this far, metres.
const SEGMENT_LENGTH: f64 = 0.25;
const MARK_WIDTH: f64 = 0.22;
const MAX_ALPHA: f64 = 0.75;

#[derive(Clone, Copy)]
struct Segment {
    from: Vec2,
    to: Vec2,
    alpha: f32,
}

pub struct SkidMarks {
    segments: Vec<Segment>,
    /// Next slot to overwrite once `segments` is full.
    next: usize,
    /// Where each wheel's current mark last ended, if it is marking.
    last: [Option<Vec2>; 4],
}

impl SkidMarks {
    pub fn new() -> Self {
        Self { segments: Vec::with_capacity(MAX_SEGMENTS), next: 0, last: [None; 4] }
    }

    pub fn track(&mut self, wheel_index: usize, at: Vec2, intensity: f64) {
        if intensity < MIN_INTENSITY {
            self.last[wheel_index] = None;
            return;
        }
        let Some(from) = self.last[wheel_index] else {
            self.last[wheel_index] = Some(at);
            return;
        };
        if from.distance(at) < SEGMENT_LENGTH {
            return;
        }
        self.push(Segment { from, to: at, alpha: (MAX_ALPHA * intensity.min(1.0)) as f32 });
        self.last[wheel_index] = Some(at);
    }

    fn push(&mut self, segment: Segment) {
        if self.segments.len() < MAX_SEGMENTS {
            self.segments.push(segment);
        } else {
            self.segments[self.next] = segment;
            self.next = (self.next + 1) % MAX_SEGMENTS;
        }
    }

    pub fn clear(&mut self) {
        self.segments.clear();
        self.next = 0;
        self.last = [None; 4];
    }

    /// Draw with the world camera active.
    pub fn draw(&self, camera: &FollowCamera) {
        let (min, max) = camera.visible();
        let visible = |p: Vec2| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
        for s in &self.segments {
            if !visible(s.from) && !visible(s.to) {
                continue;
            }
            let side = (s.to - s.from).normalize_or_zero().perp() * (MARK_WIDTH / 2.0);
            let [a, b, c, d] = [s.from + side, s.from - side, s.to - side, s.to + side].map(to_mq);
            let color = Color { a: s.alpha, ..theme::SKID };
            draw_triangle(a, b, c, color);
            draw_triangle(a, c, d, color);
        }
    }
}
