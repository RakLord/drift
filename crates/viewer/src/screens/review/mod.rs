//! Human review (`--review <run>`): the run's drivers side by side on a
//! 4×3 grid, the 8 fittest and 4 best drifters from `top.genomes`, each
//! re-run live from the same rolling start exactly as in training (like the
//! watch screen, one camera per car). Pick the ones you like and send them:
//! the trainer keeps each unchanged for a while and floods its island with
//! its children (`[review]` in training.toml, `agent::picks`).
//!
//! Click a tile for a full view of that car (dash, effects, driver info);
//! Esc goes back to the grid. Opened with `--review`, `U` on the watch
//! screen or while following a run, or the launcher's run menu. A newer
//! `top.genomes` waits for the next heat, and for unsent picks to be sent
//! or cleared.

mod grid;
mod pick;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use agent::top_file::{self, TopGenomes};
use macroquad::prelude::*;
use sim::episode::TICK;
use sim::track::generator::GeneratorParams;
use sim::track::{Track, TrackDef};

use super::watch::driver_info::DriverInfo;
use super::watch::racer::Racer;
use super::watch::{DEFAULT_TRACK, TrackChoice, build, first_fixed_track, island_colour, next_library_track, roof_of};
use crate::paths::workspace_file;
use crate::render::effects::Effects;
use crate::render::score::ScoreHud;
use crate::render::{self, camera::FollowCamera, ground, hud, text, theme, track::TrackMesh};
use crate::ui::{ButtonStyle, button};
use grid::Marks;
use pick::Picks;

const MAX_TICKS_PER_FRAME: u32 = 12;
const TIME_SCALES: [f64; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
const NORMAL_SPEED: usize = 3;
const POLL_SECS: f64 = 0.5;
/// Checking for the run's trainer reads /proc, so less often.
const TRAINING_CHECK_SECS: f64 = 3.0;
const HEAT_PAUSE_SECS: f64 = 2.0;
/// Same episode seed every heat, as on the watch screen.
const SEED: u64 = 1;
/// Room at the bottom for the status line, help and the send button.
const STRIP_H: f32 = 52.0;
/// Tiles start zoomed out a little, to show more of the track.
const TILE_ZOOM: f64 = 0.6;
const SEND_W: f32 = 190.0;

pub struct ReviewScreen {
    run: String,
    run_dir: PathBuf,
    top: Option<TopGenomes>,
    /// Newer genomes, waiting for the next heat.
    pending: Option<TopGenomes>,
    modified: Option<SystemTime>,
    poll_timer: f64,
    training_timer: f64,
    /// The run's trainer is running.
    training: bool,
    choice: TrackChoice,
    generator: GeneratorParams,
    track: Arc<Track>,
    mesh: TrackMesh,
    racers: Vec<Racer>,
    /// One per racer.
    cameras: Vec<FollowCamera>,
    /// The racer shown full screen, if any.
    zoomed: Option<usize>,
    camera: FollowCamera,
    /// Smoke, sparks and damage of the zoomed car.
    effects: Effects,
    score_hud: ScoreHud,
    driver_info: DriverInfo,
    picks: Picks,
    accumulator: f64,
    time_scale_index: usize,
    paused: bool,
    heat_over: f64,
    heats: u32,
    show_help: bool,
    toast: Option<(String, f64)>,
    switch: Option<super::Switch>,
}

impl ReviewScreen {
    pub fn new(run: &str) -> Self {
        let run_dir = workspace_file("runs").join(run);
        let config = run_dir.join("config");
        let generator = common::config::load(config.join("generator.toml"))
            .or_else(|_| common::config::load(workspace_file("config/generator.toml")))
            .unwrap_or_default();
        let choice = TrackChoice::Library(first_fixed_track(&config.join("training.toml")).unwrap_or_else(|| workspace_file(DEFAULT_TRACK)));
        let mut toast = None;
        let track = build(&choice, &generator).unwrap_or_else(|err| {
            toast = Some((err, 8.0));
            let def = TrackDef::load(workspace_file(DEFAULT_TRACK)).expect("the default track loads");
            Arc::new(Track::build(def).expect("the default track builds"))
        });
        let mut effects = Effects::new();
        effects.track_skids = false;
        let mut driver_info = DriverInfo::new();
        driver_info.open = true;
        let mut screen = Self {
            run: run.to_owned(),
            training: common::procs::is_training(run),
            run_dir,
            top: None,
            pending: None,
            modified: None,
            poll_timer: 0.0,
            training_timer: 0.0,
            choice,
            generator,
            mesh: TrackMesh::new(&track),
            camera: FollowCamera::new(track.start().0),
            track,
            racers: vec![],
            cameras: vec![],
            zoomed: None,
            effects,
            score_hud: ScoreHud::new(),
            driver_info,
            picks: Picks::default(),
            accumulator: 0.0,
            time_scale_index: NORMAL_SPEED,
            paused: false,
            heat_over: 0.0,
            heats: 0,
            show_help: true,
            toast,
            switch: None,
        };
        screen.poll(POLL_SECS);
        screen.start_heat();
        screen
    }

    /// Another view asked for (Q top cars, J replays), if any.
    pub fn take_switch(&mut self) -> Option<super::Switch> {
        self.switch.take()
    }

    pub fn notify(&mut self, message: String) {
        self.toast = Some((message, 5.0));
    }

    /// Esc: from a full view back to the grid. False when already there.
    pub fn back(&mut self) -> bool {
        self.zoomed.take().is_some()
    }

    fn top_path(&self) -> PathBuf {
        self.run_dir.join("top.genomes")
    }

    fn poll(&mut self, dt: f64) {
        self.training_timer += dt;
        if self.training_timer >= TRAINING_CHECK_SECS {
            self.training_timer = 0.0;
            self.training = common::procs::is_training(&self.run);
        }
        self.poll_timer += dt;
        if self.poll_timer < POLL_SECS {
            return;
        }
        self.poll_timer = 0.0;
        self.picks.poll(&self.run_dir);
        let modified = std::fs::metadata(self.top_path()).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.modified {
            return;
        }
        self.modified = modified;
        match top_file::load(self.top_path()) {
            Ok(top) => self.pending = Some(top),
            Err(err) => self.toast = Some((format!("{}: {err}", self.top_path().display()), 6.0)),
        }
    }

    /// Everyone back to the start, with the newest genomes unless picks
    /// from these are still unsent.
    fn start_heat(&mut self) {
        if self.picks.picked.is_empty()
            && let Some(top) = self.pending.take()
        {
            self.top = Some(top);
        }
        let zoomed_id = self.zoomed.and_then(|i| self.racers.get(i)).map(|r| r.entry.id);
        self.racers.clear();
        if let Some(top) = &self.top {
            for i in pick::choose(&top.header.entries) {
                let racer = Racer::new(top.header.entries[i].clone(), &top.genomes[i], &top.header, self.track.clone(), SEED);
                self.racers.push(racer);
            }
        }
        self.cameras = self
            .racers
            .iter()
            .map(|r| {
                let mut camera = FollowCamera::new(r.episode().world.car.state.position);
                camera.zoom_by(TILE_ZOOM);
                camera
            })
            .collect();
        self.zoomed = zoomed_id.and_then(|id| self.racers.iter().position(|r| r.entry.id == id));
        self.effects.clear();
        self.score_hud.clear();
        self.heat_over = 0.0;
        self.accumulator = 0.0;
        self.heats += 1;
    }

    fn zoom(&mut self, index: usize) {
        let Some(r) = self.racers.get(index) else { return };
        self.camera.snap_to(r.episode().world.car.state.position);
        self.zoomed = Some(index);
        self.effects.clear();
        self.score_hud.clear();
    }

    fn set_track(&mut self, choice: TrackChoice) {
        match build(&choice, &self.generator) {
            Ok(track) => {
                self.toast = Some((track.def.name.clone(), 2.0));
                self.mesh = TrackMesh::new(&track);
                self.track = track;
                self.choice = choice;
                self.start_heat();
            }
            Err(err) => self.toast = Some((err, 6.0)),
        }
    }

    fn send(&mut self) {
        let Some(top) = &self.top else { return };
        let message = match self.picks.send(&self.run_dir, top) {
            Ok(sent) if self.training => sent,
            Ok(sent) => format!("{sent}: the trainer isn't running, they apply when the run resumes"),
            Err(err) => err,
        };
        self.toast = Some((message, 4.0));
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        self.handle_keys();
        self.poll(dt);
        self.score_hud.update(dt);
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }
        if self.racers.is_empty() {
            if self.pending.is_some() {
                self.start_heat();
            }
            return;
        }
        if self.paused {
            return;
        }
        if self.racers.iter().all(|r| r.end.is_some()) {
            self.heat_over += dt;
            if self.heat_over >= HEAT_PAUSE_SECS {
                self.start_heat();
            }
        }
        self.accumulator += dt * TIME_SCALES[self.time_scale_index];
        let mut ticks = 0;
        while self.accumulator >= TICK && ticks < MAX_TICKS_PER_FRAME {
            for (i, racer) in self.racers.iter_mut().enumerate() {
                let was_running = racer.end.is_none();
                racer.tick();
                if self.zoomed == Some(i) && was_running {
                    let world = &racer.episode().world;
                    self.effects.on_tick(&world.car.state, &world.car.params, world.contacts(), TICK);
                    self.score_hud.on_events(racer.episode().scorer.events());
                }
            }
            self.accumulator -= TICK;
            ticks += 1;
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }
        let alpha = self.alpha();
        for (r, camera) in self.racers.iter().zip(&mut self.cameras) {
            let state = r.shown_state(alpha);
            camera.update(state.position, state.velocity, dt);
        }
        if let Some(r) = self.zoomed.and_then(|i| self.racers.get(i)) {
            let state = r.shown_state(alpha);
            self.camera.set_view_width(screen_width());
            self.camera.update(state.position, state.velocity, dt);
        }
    }

    fn alpha(&self) -> f64 {
        (self.accumulator / TICK).clamp(0.0, 1.0)
    }

    fn tiles(&self) -> Vec<Rect> {
        grid::layout(self.racers.len(), Rect::new(0.0, 0.0, screen_width(), screen_height() - STRIP_H))
    }

    /// The tile under the mouse, on the grid.
    fn hovered_tile(&self) -> Option<usize> {
        if self.zoomed.is_some() {
            return None;
        }
        let (mx, my) = mouse_position();
        self.tiles().iter().position(|t| t.contains(vec2(mx, my)))
    }

    fn handle_keys(&mut self) {
        let hovered = self.hovered_tile();
        let over_info = self.zoomed.is_some() && self.driver_info.hovered();
        let id_of = |i: usize, racers: &[Racer]| racers.get(i).map(|r| r.entry.id);
        let target = self.zoomed.or(hovered);
        if (is_key_pressed(KeyCode::P) || (is_mouse_button_pressed(MouseButton::Right) && !over_info))
            && let Some(id) = target.and_then(|i| id_of(i, &self.racers))
        {
            self.picks.toggle(id);
        }
        if is_mouse_button_pressed(MouseButton::Left)
            && let Some(i) = hovered
        {
            self.zoom(i);
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            self.send();
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.picks.picked.clear();
        }
        if is_key_pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
        if is_key_pressed(KeyCode::R) {
            self.start_heat();
        }
        if is_key_pressed(KeyCode::Q) {
            self.switch = Some(super::Switch::Watch(self.run.clone()));
        }
        if is_key_pressed(KeyCode::J) {
            self.switch = Some(super::Switch::Replays(self.run.clone()));
        }
        if is_key_pressed(KeyCode::L)
            && let Some(path) = next_library_track(&self.choice)
        {
            self.set_track(TrackChoice::Library(path));
        }
        if is_key_pressed(KeyCode::N) {
            let seed = match self.choice {
                TrackChoice::Generated(seed) => seed + 1,
                TrackChoice::Library(_) => 1,
            };
            self.set_track(TrackChoice::Generated(seed));
        }
        if is_key_pressed(KeyCode::D) {
            self.driver_info.open = !self.driver_info.open;
        }
        if is_key_pressed(KeyCode::H) {
            self.show_help = !self.show_help;
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.time_scale_index = self.time_scale_index.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.time_scale_index = (self.time_scale_index + 1).min(TIME_SCALES.len() - 1);
        }
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 && !over_info {
            let factor = if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 };
            match self.zoomed {
                Some(_) => self.camera.zoom_by(factor),
                None => self.cameras.iter_mut().for_each(|c| c.zoom_by(factor)),
            }
        }
    }

    fn marks(&self, r: &Racer, hovered: bool) -> Marks {
        Marks { picked: self.picks.is_picked(r.entry.id), sent: self.picks.sent_state(r.entry.id), hovered }
    }

    pub fn draw(&mut self) {
        match self.zoomed {
            Some(i) => self.draw_zoomed(i),
            None => {
                clear_background(theme::PANEL_SOLID);
                let alpha = self.alpha();
                let hovered = self.hovered_tile();
                for (i, rect) in self.tiles().into_iter().enumerate() {
                    let marks = self.marks(&self.racers[i], hovered == Some(i));
                    grid::draw(&mut self.cameras[i], rect, &self.mesh, &self.racers[i], alpha, &marks);
                }
                if self.racers.is_empty() {
                    let line = format!("waiting for {}", self.top_path().display());
                    text(&line, 16.0, screen_height() / 2.0, 22.0, theme::TEXT_DIM);
                }
            }
        }
        self.draw_status();
    }

    fn draw_zoomed(&mut self, i: usize) {
        let alpha = self.alpha();
        let r = &self.racers[i];
        let world = &r.episode().world;
        let params = &world.car.params;
        let state = r.shown_state(alpha);
        let paint = island_colour(r.entry.island);
        self.camera.set_view_width(screen_width());
        self.camera.apply();
        ground::draw(&self.camera);
        self.mesh.draw(&self.camera);
        self.mesh.draw_zones(&r.episode().scorer.zone_lit);
        r.skids.draw(&self.camera);
        self.effects.draw_ground(&self.camera, params);
        let fade = if r.end.is_some() { 0.35 } else { 1.0 };
        render::car::draw_painted(&state, params, &self.effects.damage, paint, roof_of(paint), fade);
        self.effects.draw_air();
        set_default_camera();

        let (w, bottom) = (screen_width(), screen_height() - STRIP_H);
        hud::draw_at(&state, params, w, bottom);
        hud::draw_track_info(world, w);
        self.score_hud.draw(&r.episode().scorer, w);
        if self.picks.is_picked(r.entry.id) {
            draw_rectangle_lines(0.0, 0.0, w, bottom, 6.0, theme::NEON_PINK);
            text("PICKED", 16.0, bottom - 16.0, 24.0, theme::NEON_PINK);
        }
        let generation = self.top.as_ref().map_or(0, |t| t.header.generation);
        let colour = |r: &Racer| island_colour(r.entry.island);
        if let Some(j) = self.driver_info.show(&self.racers, i, generation, w, colour) {
            self.zoom(j);
        }
    }

    fn draw_status(&mut self) {
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, h - STRIP_H, w, STRIP_H, theme::PANEL_SOLID);
        let mut parts = vec![];
        if self.paused {
            parts.push("PAUSED".to_owned());
        }
        let scale = TIME_SCALES[self.time_scale_index];
        if scale != 1.0 {
            parts.push(format!("{scale}x"));
        }
        if let Some(top) = &self.top {
            let driving = self.racers.iter().filter(|r| r.end.is_none()).count();
            parts.push(format!(
                "REVIEW {} gen {} ({})   heat {}: {driving}/{} driving   on {}",
                self.run, top.header.generation, top.header.stage, self.heats, self.racers.len(), self.track.def.name
            ));
        } else {
            parts.push(format!("REVIEW {}", self.run));
        }
        if !self.picks.picked.is_empty() {
            parts.push(format!("{} picked", self.picks.picked.len()));
        }
        if let Some(latest) = self.picks.latest() {
            parts.push(latest);
        }
        if !self.training {
            parts.push("trainer not running: picks apply on resume".to_owned());
        }
        if let Some(next) = &self.pending {
            let wait = if self.picks.picked.is_empty() { "" } else { " (send or clear your picks first)" };
            parts.push(format!("next heat: gen {}{wait}", next.header.generation));
        }
        let room = w - SEND_W - 48.0;
        let line = parts.join("   ");
        let natural = measure_text(&line, None, 20, 1.0).width;
        let size = (20.0 * room / natural).clamp(11.0, 20.0);
        text(&line, 16.0, h - 34.0, size, theme::SODIUM);

        let help = match (self.show_help, self.zoomed.is_some()) {
            (false, _) => "H: help",
            (true, false) => {
                "click a car: full view  P / right-click: pick  Enter: send picks  Backspace: clear picks  R restart heat  L library track  N generated track  Q top cars  J replays  Space pause  [ ] speed  wheel zoom  H help"
            }
            (true, true) => {
                "Esc: back to the grid  P / right-click: pick  Enter: send picks  Backspace: clear picks  D driver info  R restart heat  Space pause  [ ] speed  wheel zoom  H help"
            }
        };
        let natural = measure_text(help, None, 15, 1.0).width;
        let size = (15.0 * room / natural).clamp(9.0, 15.0);
        text(help, 16.0, h - 12.0, size, theme::TEXT_DIM);

        let count = self.picks.picked.len();
        if count > 0 && button(Rect::new(w - SEND_W - 16.0, h - STRIP_H + 9.0, SEND_W, 34.0), &format!("Send {count} pick{}", if count == 1 { "" } else { "s" }), ButtonStyle::Primary) {
            self.send();
        }
        if let Some((message, _)) = &self.toast {
            let width = measure_text(message, None, 22, 1.0).width;
            text(message, ((w - width) / 2.0).max(16.0), 170.0, 22.0, theme::NEON_CYAN);
        }
    }
}
