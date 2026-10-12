//! Generator panel (F): sliders for the track generator, plus buttons to
//! regenerate, roll a new seed, keep the track, or save the sliders as the
//! defaults in `config/generator.toml`.

use macroquad::prelude::*;
use sim::track::generator::GeneratorParams;

use crate::paths::workspace_file;
use crate::render::theme;
use crate::ui::{ButtonStyle, Panel};

pub enum PanelAction {
    Generate,
    Keep,
    SaveDefaults,
}

pub struct GeneratorPanel {
    pub open: bool,
    /// Why the last generate failed, if it did (shown under the seed).
    pub problem: Vec<String>,
    panel: Panel,
}

impl GeneratorPanel {
    pub fn new(open: bool) -> Self {
        let mut panel = Panel::new(16.0, 110.0, 400.0);
        // Less-used sections start folded away.
        panel.collapse("Shapes");
        panel.collapse("Zones");
        Self { open, problem: vec![], panel }
    }

    /// Whether the mouse is over the panel (the camera shouldn't zoom).
    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Draw the panel (if open), handling its input. Returns what was clicked.
    pub fn show(&mut self, p: &mut GeneratorParams, seed: &mut u64) -> Option<PanelAction> {
        if !self.open {
            return None;
        }
        let ui = &mut self.panel;
        let problem = std::mem::take(&mut self.problem);
        let mut action = None;
        ui.begin("TRACK GENERATOR", screen_height() - 16.0);

        ui.label(&format!("seed {seed}"), theme::SODIUM);
        for line in &problem {
            ui.label(line, theme::SATURATED);
        }
        match ui.buttons(&[("< prev", ButtonStyle::Normal), ("random", ButtonStyle::Normal), ("next >", ButtonStyle::Normal)]) {
            Some(0) => {
                *seed = seed.saturating_sub(1);
                action = Some(PanelAction::Generate);
            }
            Some(1) => {
                *seed = scramble(*seed);
                action = Some(PanelAction::Generate);
            }
            Some(2) => {
                *seed += 1;
                action = Some(PanelAction::Generate);
            }
            _ => {}
        }
        match ui.buttons(&[("GENERATE", ButtonStyle::Primary), ("keep", ButtonStyle::Normal), ("save defaults", ButtonStyle::Normal)]) {
            Some(0) => action = Some(PanelAction::Generate),
            Some(1) => action = Some(PanelAction::Keep),
            Some(2) => action = Some(PanelAction::SaveDefaults),
            _ => {}
        }

        let l = &mut p.layout;
        if ui.heading("Layout") {
            ui.slider("length min (m)", &mut l.length_min, 300.0..=4000.0, 0);
            ui.slider("length max (m)", &mut l.length_max, 300.0..=5000.0, 0);
            ui.int_slider("pieces min", &mut l.pieces_min, 1..=20);
            ui.int_slider("pieces max", &mut l.pieces_max, 1..=24);
            ui.slider("straight chance", &mut l.straight_chance, 0.0..=1.0, 2);
            ui.slider("homing start", &mut l.homing, 0.0..=1.0, 2);
            ui.slider("homing end", &mut l.homing_end, 0.0..=1.0, 2);
            ui.slider("figure-8", &mut l.figure8, 0.0..=1.0, 2);
            ui.int_slider("min crossings", &mut l.min_crossings, 0..=3);
            ui.int_slider("max crossings", &mut l.max_crossings, 0..=3);
            ui.slider("closing radius (m)", &mut l.closing_radius, 20.0..=120.0, 0);
            ui.slider("max closing straight (m)", &mut l.max_closing_straight, 0.0..=400.0, 0);
            ui.slider("max closing turn (deg)", &mut l.max_closing_turn, 90.0..=360.0, 0);
            ui.int_slider("return pieces", &mut l.return_pieces, 0..=8);
        }

        let w = &mut p.pieces;
        if ui.heading("Pieces") {
            ui.slider("hairpin", &mut w.hairpin, 0.0..=5.0, 1);
            ui.slider("sweeper", &mut w.sweeper, 0.0..=5.0, 1);
            ui.slider("s-bend", &mut w.s_bend, 0.0..=5.0, 1);
            ui.slider("decreasing radius", &mut w.decreasing, 0.0..=5.0, 1);
            ui.slider("chicane / manji", &mut w.chicane, 0.0..=5.0, 1);
            ui.slider("kink", &mut w.kink, 0.0..=5.0, 1);
        }

        let s = &mut p.shapes;
        if ui.heading("Shapes") {
            range_sliders(ui, "hairpin radius", &mut s.hairpin_radius, 15.0..=60.0);
            range_sliders(ui, "hairpin angle", &mut s.hairpin_angle, 90.0..=240.0);
            range_sliders(ui, "sweeper radius", &mut s.sweeper_radius, 30.0..=200.0);
            range_sliders(ui, "sweeper angle", &mut s.sweeper_angle, 20.0..=180.0);
            range_sliders(ui, "s-bend radius", &mut s.s_bend_radius, 20.0..=120.0);
            range_sliders(ui, "s-bend angle", &mut s.s_bend_angle, 20.0..=150.0);
            range_sliders(ui, "chicane radius", &mut s.chicane_radius, 20.0..=100.0);
            range_sliders(ui, "chicane angle", &mut s.chicane_angle, 10.0..=90.0);
            range_sliders(ui, "kink radius", &mut s.kink_radius, 40.0..=250.0);
            range_sliders(ui, "kink angle", &mut s.kink_angle, 5.0..=60.0);
        }

        let f = &mut p.surface;
        if ui.heading("Surface") {
            ui.slider("width min (m)", &mut f.width[0], 8.0..=25.0, 1);
            ui.slider("width max (m)", &mut f.width[1], 8.0..=25.0, 1);
            ui.slider("verge (m)", &mut f.verge, 2.0..=15.0, 1);
            ui.slider("wall outside", &mut f.outside_hard_chance, 0.0..=1.0, 2);
            ui.slider("wall inside", &mut f.inside_hard_chance, 0.0..=1.0, 2);
            ui.slider("wall on straights", &mut f.straight_hard_chance, 0.0..=1.0, 2);
        }

        let z = &mut p.zones;
        if ui.heading("Zones") {
            ui.slider("clip min angle (deg)", &mut z.clip_min_angle, 0.0..=180.0, 0);
            ui.slider("outer min angle (deg)", &mut z.outer_min_angle, 0.0..=180.0, 0);
        }

        ui.end();
        self.problem = problem;
        action
    }
}

/// A [min, max] pair as two sliders.
fn range_sliders(ui: &mut Panel, name: &str, pair: &mut [f64; 2], range: std::ops::RangeInclusive<f64>) {
    ui.slider(&format!("{name} min"), &mut pair[0], range.clone(), 0);
    ui.slider(&format!("{name} max"), &mut pair[1], range, 0);
}

/// A new pseudo-random seed from the current one (splitmix64).
fn scramble(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) % 1_000_000
}

/// Write the panel's values to `config/generator.toml`.
pub fn save_defaults(params: &GeneratorParams) -> Result<String, String> {
    let path = workspace_file("config/generator.toml");
    let header = "# Procedural drift tracks. Saved from the viewer's generator panel (F).\n\
                  # Field meanings: crates/sim/src/track/generator/mod.rs and the panel labels.\n\n";
    let body = toml::to_string_pretty(params).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{header}{body}")).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.display().to_string())
}
