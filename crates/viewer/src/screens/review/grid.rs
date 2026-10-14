//! The review grid's tiles: where they sit, and one car drawn in each with
//! its own follow camera.

use agent::picks::State;
use macroquad::prelude::*;

use super::super::watch::{island_colour, roof_of};
use super::super::watch::racer::Racer;
use crate::render::camera::FollowCamera;
use crate::render::effects::damage::Damage;
use crate::render::track::TrackMesh;
use crate::render::{self, ground, text, theme};
use crate::ui::{chip, fit};

const GAP: f32 = 4.0;
const LABEL: f32 = 14.0;

/// `count` tiles in a near-square grid filling `area`, row by row.
pub fn layout(count: usize, area: Rect) -> Vec<Rect> {
    if count == 0 {
        return vec![];
    }
    let cols = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(cols);
    let w = (area.w - GAP * (cols as f32 - 1.0)) / cols as f32;
    let h = (area.h - GAP * (rows as f32 - 1.0)) / rows as f32;
    (0..count)
        .map(|i| {
            let (col, row) = ((i % cols) as f32, (i / cols) as f32);
            Rect::new(area.x + col * (w + GAP), area.y + row * (h + GAP), w, h)
        })
        .collect()
}

/// How a tile's car stands with the review.
pub struct Marks {
    pub picked: bool,
    pub sent: Option<State>,
    pub hovered: bool,
}

/// The track, the car's skid marks and the car, then its labels on top.
pub fn draw(camera: &mut FollowCamera, rect: Rect, mesh: &TrackMesh, racer: &Racer, alpha: f64, marks: &Marks) {
    camera.set_tile(rect);
    camera.apply();
    ground::draw_in_view(camera);
    mesh.draw(camera);
    mesh.draw_zones(&racer.episode().scorer.zone_lit);
    racer.skids.draw(camera);
    let state = racer.shown_state(alpha);
    let paint = island_colour(racer.entry.island);
    let fade = if racer.end.is_some() { 0.35 } else { 1.0 };
    render::car::draw_painted(&state, &racer.episode().world.car.params, &Damage::new(), paint, roof_of(paint), fade);
    set_default_camera();

    let (border, thickness) = match (marks.picked, marks.hovered) {
        (true, _) => (theme::NEON_PINK, 4.0),
        (false, true) => (theme::TEXT, 2.0),
        (false, false) => (paint, 1.5),
    };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, thickness, border);
    draw_rectangle(rect.x, rect.y, rect.w, LABEL + 8.0, theme::PANEL);
    let points = format!("{:.0} pts", racer.live_score());
    let points_w = measure_text(&points, None, LABEL as u16, 1.0).width;
    let label = fit(&format!("{}  island {}", racer.label, racer.entry.island), rect.w - points_w - 20.0, LABEL);
    text(&label, rect.x + 6.0, rect.y + LABEL + 2.0, LABEL, if marks.picked { theme::NEON_PINK } else { theme::TEXT });
    text(&points, rect.x + rect.w - points_w - 6.0, rect.y + LABEL + 2.0, LABEL, theme::SODIUM);

    let status = match racer.end {
        Some(end) => end.name().to_owned(),
        None => format!("{:.0}°", state.body_slip_angle().to_degrees().abs()),
    };
    text(&status, rect.x + 6.0, rect.y + rect.h - 6.0, LABEL, theme::TEXT_DIM);
    let tag = match (marks.picked, marks.sent) {
        (true, _) => Some(("PICKED", theme::NEON_PINK)),
        (false, Some(State::Waiting)) => Some(("SENT", theme::SODIUM)),
        (false, Some(State::Applied(_))) => Some(("BOOSTED", theme::GRIP)),
        (false, Some(State::Rejected | State::Gone) | None) => None,
    };
    if let Some((tag, colour)) = tag {
        let w = measure_text(tag, None, 14, 1.0).width + 10.0;
        chip(rect.x + rect.w - w - 6.0, rect.y + rect.h - 24.0, tag, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_tiles_make_four_across_three_down() {
        let tiles = layout(12, Rect::new(0.0, 0.0, 1600.0, 900.0));
        assert_eq!(tiles.len(), 12);
        let last = tiles[11];
        assert!((last.x + last.w - 1600.0).abs() < 1e-3 && (last.y + last.h - 900.0).abs() < 1e-3, "fills the area");
        assert_eq!((tiles[3].y, tiles[4].y), (0.0, tiles[0].h + GAP), "four a row");
        assert_eq!(tiles[11].y, 2.0 * (tiles[0].h + GAP), "three rows");
    }

    #[test]
    fn fewer_tiles_stay_near_square() {
        let tiles = layout(7, Rect::new(0.0, 0.0, 900.0, 900.0));
        assert_eq!(tiles.len(), 7);
        assert_eq!(tiles[2].y, 0.0);
        assert!(tiles[3].y > 0.0, "three a row");
    }
}
