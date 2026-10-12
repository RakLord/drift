//! Leaderboard (live multi-car view): every car, best live score first, with
//! its island colour, drift angle and how its run is going. Click a row to
//! follow that car.

use macroquad::prelude::*;

use super::racer::Racer;
use crate::render::theme;
use crate::ui::Panel;

pub struct Leaderboard {
    pub open: bool,
    panel: Panel,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self { open: true, panel: Panel::new(16.0, 110.0, 420.0) }
    }

    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Draw (if open); returns the racer clicked, if any.
    pub fn show(&mut self, racers: &[Racer], followed: usize, colour: impl Fn(&Racer) -> Color) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut order: Vec<usize> = (0..racers.len()).collect();
        order.sort_by(|&a, &b| racers[b].live_score().total_cmp(&racers[a].live_score()));

        let ui = &mut self.panel;
        let driving = racers.iter().filter(|r| r.end.is_none()).count();
        ui.begin("LEADERBOARD", screen_height() - 140.0);
        ui.label(&format!("{driving} of {} driving   click to follow", racers.len()), theme::TEXT_DIM);
        let mut clicked = None;
        for (place, &i) in order.iter().enumerate() {
            let r = &racers[i];
            let state = &r.episode().world.car.state;
            let angle = if state.speed() > 3.0 { state.body_slip_angle().abs().to_degrees() } else { 0.0 };
            let status = r.end.map_or_else(|| format!("{angle:>3.0}°"), |end| end.name().to_owned());
            let detail = format!("{:>6.0} pts  {status}", r.live_score());
            let label = format!("{}. {}", place + 1, r.label);
            if ui.list_item_swatch(&label, &detail, i == followed, colour(r)) {
                clicked = Some(i);
            }
        }
        ui.end();
        clicked
    }
}
