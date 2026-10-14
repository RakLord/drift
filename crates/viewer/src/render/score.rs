//! Score HUD: banked total with its Line/Angle/Style split, the live combo,
//! and popups for scoring events.

use common::Vec2;
use macroquad::prelude::*;
use sim::scoring::{ScoreEvent, ScoreEventKind, Scorer};

use super::{light, text, theme};

/// The neon glow behind a running combo: radius (pixels) and strength,
/// growing with the multiplier.
const COMBO_GLOW_RADIUS: f64 = 130.0;
const COMBO_GLOW: (f32, f32) = (0.10, 0.30);

const POPUP_SECS: f64 = 1.6;
const POPUP_RISE: f32 = 40.0;
const MAX_POPUPS: usize = 6;

struct Popup {
    text: String,
    color: Color,
    age: f64,
}

pub struct ScoreHud {
    popups: Vec<Popup>,
}

impl ScoreHud {
    pub fn new() -> Self {
        Self { popups: vec![] }
    }

    pub fn clear(&mut self) {
        self.popups.clear();
    }

    pub fn on_events(&mut self, events: &[ScoreEvent]) {
        for e in events {
            if let Some((text, color)) = describe(e) {
                if self.popups.len() == MAX_POPUPS {
                    self.popups.remove(0);
                }
                self.popups.push(Popup { text, color, age: 0.0 });
            }
        }
    }

    pub fn update(&mut self, dt: f64) {
        for p in &mut self.popups {
            p.age += dt;
        }
        self.popups.retain(|p| p.age < POPUP_SECS);
    }

    pub fn draw(&self, scorer: &Scorer, view_w: f32) {
        let cx = view_w / 2.0;

        let total = scorer.total;
        centred(&format_points(total.total()), cx, 70.0, 48.0, theme::TEXT);
        let split = format!(
            "line {}   angle {}   style {}   pen -{}",
            format_points(total.line),
            format_points(total.angle),
            format_points(total.style),
            format_points(total.penalty)
        );
        centred(&split, cx, 92.0, 16.0, theme::TEXT_DIM);

        let combo = &scorer.combo;
        if combo.active {
            let value = combo.value().total();
            let line = format!("+{}  x{:.1}", format_points(value), combo.multiplier);
            // x1 to the multiplier cap: dim to full glow.
            let cap = scorer.params.combo.multiplier_max;
            let heat = ((combo.multiplier - 1.0) / (cap - 1.0).max(1e-9)).clamp(0.0, 1.0) as f32;
            let strength = COMBO_GLOW.0 + (COMBO_GLOW.1 - COMBO_GLOW.0) * heat;
            light::pass(|| light::glow(Vec2::new(f64::from(cx), 116.0), COMBO_GLOW_RADIUS, Color { a: strength, ..theme::NEON_PINK }));
            centred(&line, cx, 128.0, 36.0, theme::NEON_PINK);
            if combo.taps > 0 {
                centred(&format!("wall taps {}", combo.taps), cx, 148.0, 16.0, theme::TEXT_DIM);
            }
        }
        if scorer.drift.active {
            let hold = scorer.params.flow.hold_credit(scorer.flow.hold);
            centred(&format!("flow x{hold:.2}"), cx, 166.0, 16.0, theme::TEXT_DIM);
        }

        for (i, p) in self.popups.iter().rev().enumerate() {
            let t = (p.age / POPUP_SECS) as f32;
            let y = 200.0 + i as f32 * 30.0 - t * POPUP_RISE;
            let color = Color { a: (1.0 - t * t).max(0.0), ..p.color };
            centred(&p.text, cx, y, 28.0, color);
        }
    }
}

fn describe(e: &ScoreEvent) -> Option<(String, Color)> {
    let pts = format_points(e.points.abs());
    Some(match e.kind {
        ScoreEventKind::DriftStart | ScoreEventKind::ClipMissed => return None,
        ScoreEventKind::Clip { quality } if quality > 0.95 => (format!("PERFECT CLIP +{pts}"), theme::SODIUM),
        ScoreEventKind::Clip { .. } => (format!("CLIP +{pts}"), theme::SODIUM),
        ScoreEventKind::ZoneEnter => ("OUTER ZONE".to_owned(), theme::NEON_PINK),
        ScoreEventKind::Transition => (format!("TRANSITION +{pts}"), theme::NEON_CYAN),
        ScoreEventKind::WallTap => (format!("WALL TAP! +{pts}"), theme::NEON_CYAN),
        ScoreEventKind::Bobble => (format!("BOBBLE -{pts}"), theme::TEXT_DIM),
        ScoreEventKind::Banked if e.points > 0.0 => (format!("BANKED +{pts}"), theme::GRIP),
        ScoreEventKind::Banked => return None,
        ScoreEventKind::Spin => (format!("SPIN -{pts}"), theme::SATURATED),
        ScoreEventKind::HardHit => (format!("CRASH -{pts}"), theme::SATURATED),
    })
}

/// Whole points with thousands separators.
fn format_points(points: f64) -> String {
    let n = points.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn centred(s: &str, cx: f32, y: f32, size: f32, color: Color) {
    let width = measure_text(s, None, size as u16, 1.0).width;
    text(s, cx - width / 2.0, y, size, color);
}

#[cfg(test)]
mod tests {
    use super::format_points;

    #[test]
    fn thousands_separators() {
        assert_eq!(format_points(0.0), "0");
        assert_eq!(format_points(999.4), "999");
        assert_eq!(format_points(1234.0), "1,234");
        assert_eq!(format_points(-1234567.0), "-1,234,567");
    }
}
