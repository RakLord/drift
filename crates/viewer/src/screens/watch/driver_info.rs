//! Driver info (D, live multi-car view): the followed car's details at the
//! right of the track view. Who it is, where it came from (parents and
//! children racing in this heat are clickable), what its training runs this
//! generation came to, and how this heat is going.

use macroquad::prelude::*;

use super::leaderboard::age;
use super::racer::Racer;
use crate::render::theme;
use crate::ui::Panel;

const WIDTH: f32 = 460.0;

pub struct DriverInfo {
    pub open: bool,
    panel: Panel,
}

impl DriverInfo {
    pub fn new() -> Self {
        Self { open: false, panel: Panel::new(0.0, 110.0, WIDTH) }
    }

    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Draw (if open) against the right edge of a view `view_w` wide.
    /// Returns a relative clicked, to follow. `generation`: the heat's.
    pub fn show(
        &mut self,
        racers: &[Racer],
        followed: usize,
        generation: u32,
        view_w: f32,
        colour: impl Fn(&Racer) -> Color,
    ) -> Option<usize> {
        let r = racers.get(followed).filter(|_| self.open)?;
        let e = &r.entry;
        let ui = &mut self.panel;
        ui.x = (view_w - WIDTH - 16.0).max(16.0);
        ui.begin("DRIVER", screen_height() - 140.0);
        ui.label(&r.label, theme::TEXT);
        ui.label(&format!("island {}   genome {}   network {}", e.island, e.id, r.shape), theme::TEXT_DIM);
        ui.label(&format!("fitness {:.0} (#{} this gen)   quality {:.2}", e.fitness, e.rank, e.drift_quality), theme::TEXT_DIM);

        let mut clicked = None;
        if ui.heading("Lineage") {
            match age(r, generation) {
                None => ui.label("not in this run's top.genomes: resume it to fill", theme::TEXT_DIM),
                Some(age) => {
                    ui.label(&format!("born gen {} ({age} gens ago)   sigma {:.3}", e.born, e.sigma), theme::TEXT_DIM);
                    if e.parents.is_empty() {
                        ui.label("no parents: a random newcomer", theme::TEXT_DIM);
                    }
                    for &id in &e.parents {
                        match racers.iter().position(|o| o.entry.id == id) {
                            Some(i) => {
                                if ui.list_item_swatch(&format!("parent {}", racers[i].label), "racing", false, colour(&racers[i])) {
                                    clicked = Some(i);
                                }
                            }
                            None => ui.label(&format!("parent {id} (not in this heat)"), theme::TEXT_DIM),
                        }
                    }
                    for (i, child) in racers.iter().enumerate().filter(|(_, o)| o.entry.parents.contains(&e.id)) {
                        if ui.list_item_swatch(&format!("child {}", child.label), "racing", false, colour(child)) {
                            clicked = Some(i);
                        }
                    }
                }
            }
        }

        if let Some(t) = &e.training
            && ui.heading("Training this gen")
        {
            ui.label(&format!("{} runs   {:.2} laps   {:.0} m (means)", t.runs, t.laps, t.progress), theme::TEXT_DIM);
            if let [driving, flow, judged] = e.objectives[..] {
                ui.label(&format!("objectives: driving {driving:.0}   flow {flow:.2}   judged {judged:.0}"), theme::TEXT_DIM);
            }
            let s = &t.score;
            ui.label(
                &format!("line {:.0}   angle {:.0}   style {:.0}   penalty {:.0}", s.line, s.angle, s.style, s.penalty),
                theme::TEXT_DIM,
            );
            ui.label(&format!("ended: {}", tally(&t.ends)), theme::TEXT_DIM);
        }

        if ui.heading("This heat") {
            let result = r.episode().result();
            let s = &result.score;
            let state = r.end.map_or("driving", |end| end.name());
            ui.label(&format!("{:.0} pts   {state}   {:.0}s", r.live_score(), result.time), theme::TEXT);
            ui.label(
                &format!("line {:.0}   angle {:.0}   style {:.0}   penalty {:.0}", s.line, s.angle, s.style, s.penalty),
                theme::TEXT_DIM,
            );
            ui.label(&format!("{:.0} m   {:.2} laps", result.progress, result.laps()), theme::TEXT_DIM);
            let st = &result.stats;
            let share = |secs: f64| if result.time > 0.0 { 100.0 * secs / result.time } else { 0.0 };
            ui.label(
                &format!("drifting {:.0}%   above 20° {:.0}%   biggest {:.0}°", share(st.drift_time), share(st.above_20_time), st.max_angle),
                theme::TEXT_DIM,
            );
            ui.label(&format!("spins {}   zones {}   clips {}   transitions {}", st.spins, st.zones_entered, st.clips, st.transitions), theme::TEXT_DIM);
        }
        ui.end();
        clicked
    }
}

/// "time up 2  ·  crashed 1", most common first.
fn tally(ends: &[String]) -> String {
    let mut counts: Vec<(&str, usize)> = vec![];
    for end in ends {
        match counts.iter_mut().find(|(name, _)| name == end) {
            Some((_, n)) => *n += 1,
            None => counts.push((end, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.iter().map(|(name, n)| format!("{name} {n}")).collect::<Vec<_>>().join("  ·  ")
}
