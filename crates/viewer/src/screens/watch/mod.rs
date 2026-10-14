//! Live multi-car view (`--watch <run>`): the run's top genomes drive one
//! track together from the same rolling start. They're ghosts (no contact
//! between cars) and each runs exactly as in training, under the rules the
//! trainer wrote into `top.genomes`.
//!
//! Cars are coloured by island. The followed car gets the full effects,
//! HUD, score popups and network view; every car leaves skid marks. When
//! every run has ended, a new heat starts, with newer genomes if training
//! has written them.

pub(super) mod driver_info;
mod leaderboard;
pub(super) mod racer;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use agent::top_file::{self, TopGenomes};
use macroquad::prelude::*;
use sim::episode::TICK;
use sim::track::generator::{self, GeneratorParams};
use sim::track::{Track, TrackDef};

use super::play::{NetView, track_bounds};
use crate::dashboard::Dashboard;
use crate::debug::network_view::NetworkView;
use crate::debug::{driver_prep_panel, inputs_panel, telemetry};
use crate::paths::workspace_file;
use crate::render::effects::damage::Damage;
use crate::render::score::ScoreHud;
use crate::render::{self, camera::FollowCamera, effects::Effects, ground, hud, light, text, theme, track::TrackMesh};
use driver_info::DriverInfo;
use leaderboard::Leaderboard;
use racer::Racer;

const MAX_TICKS_PER_FRAME: u32 = 12;
const TIME_SCALES: [f64; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
const NORMAL_SPEED: usize = 3;
const POLL_SECS: f64 = 0.5;
/// Pause between heats, seconds.
const HEAT_PAUSE_SECS: f64 = 2.0;
/// Same episode seed every heat, so heats on a track are comparable.
const SEED: u64 = 1;
const SPLIT_TRACK_SHARE: f32 = 0.45;
/// Room at the bottom for the status line and help; the HUD sits above it.
const STRIP_H: f32 = 52.0;
/// The pack's headlights are dimmed to this (30 overlapping beams would
/// flood the road); the followed car's are full strength.
const PACK_BEAMS: f32 = 0.3;
/// Clicking within this many metres of a car follows it.
const CLICK_RADIUS: f64 = 5.0;
pub(super) const DEFAULT_TRACK: &str = "config/default_track.toml";

/// One colour per island: the validated categorical palette, in order.
const ISLAND_COLOURS: [Color; 6] = [
    Color::new(0x39 as f32 / 255.0, 0x87 as f32 / 255.0, 0xe5 as f32 / 255.0, 1.0),
    Color::new(0xd9 as f32 / 255.0, 0x59 as f32 / 255.0, 0x26 as f32 / 255.0, 1.0),
    Color::new(0x19 as f32 / 255.0, 0x9e as f32 / 255.0, 0x70 as f32 / 255.0, 1.0),
    Color::new(0xc9 as f32 / 255.0, 0x85 as f32 / 255.0, 0x00 as f32 / 255.0, 1.0),
    Color::new(0xd5 as f32 / 255.0, 0x51 as f32 / 255.0, 0x81 as f32 / 255.0, 1.0),
    Color::new(0x00 as f32 / 255.0, 0x83 as f32 / 255.0, 0x00 as f32 / 255.0, 1.0),
];

pub(super) fn island_colour(island: u32) -> Color {
    ISLAND_COLOURS[island as usize % ISLAND_COLOURS.len()]
}

/// A slightly darker shade for the roof.
pub(super) fn roof_of(c: Color) -> Color {
    Color::new(c.r * 0.8, c.g * 0.8, c.b * 0.8, c.a)
}

#[derive(Clone, PartialEq)]
pub(super) enum TrackChoice {
    Library(PathBuf),
    Generated(u64),
}

pub struct WatchScreen {
    run: String,
    run_dir: PathBuf,
    /// The genomes in the current heat.
    top: Option<TopGenomes>,
    /// Newer genomes, waiting for the next heat.
    pending: Option<TopGenomes>,
    modified: Option<SystemTime>,
    poll_timer: f64,
    choice: TrackChoice,
    generator: GeneratorParams,
    track: Arc<Track>,
    mesh: TrackMesh,
    racers: Vec<Racer>,
    followed: usize,
    /// Smoke, sparks and damage of the followed car.
    effects: Effects,
    no_damage: Damage,
    score_hud: ScoreHud,
    seen_decisions: u64,
    network_view: NetworkView,
    net_view: NetView,
    leaderboard: Leaderboard,
    driver_info: DriverInfo,
    dashboard: Dashboard,
    camera: FollowCamera,
    accumulator: f64,
    time_scale_index: usize,
    paused: bool,
    /// Seconds since every run ended.
    heat_over: f64,
    heats: u32,
    map_view: bool,
    show_effects: bool,
    show_telemetry: bool,
    show_inputs: bool,
    show_help: bool,
    toast: Option<(String, f64)>,
    switch: Option<super::Switch>,
}

impl WatchScreen {
    pub fn new(run: &str, charts: bool) -> Self {
        let run_dir = workspace_file("runs").join(run);
        let config = run_dir.join("config");
        let generator = common::config::load(config.join("generator.toml"))
            .or_else(|_| common::config::load(workspace_file("config/generator.toml")))
            .unwrap_or_default();
        let fixed = first_fixed_track(&config.join("training.toml"));
        let mut warnings = vec![];
        let choice = TrackChoice::Library(fixed.unwrap_or_else(|| workspace_file(DEFAULT_TRACK)));
        let track = build(&choice, &generator).unwrap_or_else(|err| {
            warnings.push(err);
            let def = TrackDef::load(workspace_file(DEFAULT_TRACK)).expect("the default track loads");
            Arc::new(Track::build(def).expect("the default track builds"))
        });
        let mut effects = Effects::new();
        effects.track_skids = false;
        let mut screen = Self {
            run: run.to_owned(),
            mesh: TrackMesh::new(&track),
            camera: FollowCamera::new(track.start().0),
            dashboard: Dashboard::new(Some(run_dir.join("metrics.csv")), charts),
            run_dir,
            top: None,
            pending: None,
            modified: None,
            poll_timer: 0.0,
            choice,
            generator,
            track,
            racers: vec![],
            followed: 0,
            effects,
            no_damage: Damage::new(),
            score_hud: ScoreHud::new(),
            seen_decisions: 0,
            network_view: NetworkView::new(),
            net_view: NetView::Off,
            leaderboard: Leaderboard::new(),
            driver_info: DriverInfo::new(),
            accumulator: 0.0,
            time_scale_index: NORMAL_SPEED,
            paused: false,
            heat_over: 0.0,
            heats: 0,
            map_view: false,
            show_effects: true,
            show_telemetry: false,
            show_inputs: false,
            show_help: true,
            toast: (!warnings.is_empty()).then(|| (warnings.join("; "), 8.0)),
            switch: None,
        };
        screen.poll(POLL_SECS);
        screen.start_heat();
        screen
    }

    /// Start the network view split beside the track (`--net`).
    pub fn open_network(&mut self) {
        self.net_view = NetView::Split;
    }

    /// Another view asked for (J: this run's replays), if any.
    pub fn take_switch(&mut self) -> Option<super::Switch> {
        self.switch.take()
    }

    pub fn notify(&mut self, message: String) {
        self.toast = Some((message, 5.0));
    }

    fn top_path(&self) -> PathBuf {
        self.run_dir.join("top.genomes")
    }

    /// Check for a newer top.genomes (every half second).
    fn poll(&mut self, dt: f64) {
        self.poll_timer += dt;
        if self.poll_timer < POLL_SECS {
            return;
        }
        self.poll_timer = 0.0;
        let modified = std::fs::metadata(self.top_path()).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.modified {
            return;
        }
        self.modified = modified;
        match top_file::load(self.top_path()) {
            Ok(top) => {
                let generation = top.header.generation;
                self.pending = Some(top);
                if !self.racers.is_empty() {
                    self.toast = Some((format!("gen {generation} waiting for the next heat"), 3.0));
                }
            }
            Err(err) => self.toast = Some((format!("{}: {err}", self.top_path().display()), 6.0)),
        }
    }

    /// Everyone back to the start (with the newest genomes, if any).
    fn start_heat(&mut self) {
        if let Some(top) = self.pending.take() {
            self.top = Some(top);
        }
        let Some(top) = &self.top else {
            self.racers.clear();
            return;
        };
        self.racers = top
            .header
            .entries
            .iter()
            .zip(&top.genomes)
            .map(|(entry, genome)| Racer::new(entry.clone(), genome, &top.header, self.track.clone(), SEED))
            .collect();
        // Follow the fittest.
        let fittest = self.racers.iter().enumerate().min_by_key(|(_, r)| r.entry.rank).map_or(0, |(i, _)| i);
        self.follow(fittest);
        self.heat_over = 0.0;
        self.accumulator = 0.0;
        self.heats += 1;
    }

    fn follow(&mut self, index: usize) {
        self.followed = index.min(self.racers.len().saturating_sub(1));
        self.effects.clear();
        self.score_hud.clear();
        self.network_view.clear();
        self.seen_decisions = 0;
        if let Some(r) = self.racers.get(self.followed) {
            self.camera.snap_to(r.episode().world.car.state.position);
        }
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

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        self.handle_keys();
        self.poll(dt);
        self.dashboard.update(dt);
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
                if i == self.followed && was_running {
                    let world = &racer.episode().world;
                    let car = &world.car;
                    self.effects.on_tick(&car.state, &car.params, world.contacts(), TICK);
                    self.score_hud.on_events(racer.episode().scorer.events());
                }
            }
            self.feed_network_view();
            self.accumulator -= TICK;
            ticks += 1;
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }

        self.camera.set_view_width(self.track_width());
        if self.map_view {
            let (min, max) = track_bounds(&self.track);
            self.camera.show_area(min, max);
        } else if let Some(r) = self.racers.get(self.followed) {
            let state = r.shown_state(self.alpha());
            self.camera.update(state.position, state.velocity, dt);
        }
    }

    fn alpha(&self) -> f64 {
        (self.accumulator / TICK).clamp(0.0, 1.0)
    }

    fn feed_network_view(&mut self) {
        if self.net_view == NetView::Off {
            return;
        }
        let Some(r) = self.racers.get(self.followed) else { return };
        let (observation, decisions) = r.last_decision();
        if decisions != self.seen_decisions
            && let Some(observation) = observation
        {
            self.network_view.on_decision(r.network(), &observation.values);
            self.seen_decisions = decisions;
        }
    }

    fn handle_keys(&mut self) {
        // The dashboard covers the screen and takes the input while open.
        if self.dashboard.open {
            if is_key_pressed(KeyCode::C) {
                self.dashboard.open = false;
            }
            return;
        }
        if is_key_pressed(KeyCode::Tab) {
            self.leaderboard.expanded = !self.leaderboard.expanded;
            self.leaderboard.open = true;
        }
        if is_key_pressed(KeyCode::G) {
            self.leaderboard.grouped = !self.leaderboard.grouped;
            self.leaderboard.open = true;
        }
        if is_key_pressed(KeyCode::F) && !self.racers.is_empty() {
            self.follow((self.followed + 1) % self.racers.len());
        }
        if is_key_pressed(KeyCode::R) {
            self.start_heat();
        }
        if is_key_pressed(KeyCode::J) {
            self.switch = Some(super::Switch::Replays(self.run.clone()));
        }
        if is_key_pressed(KeyCode::U) {
            self.switch = Some(super::Switch::Review(self.run.clone()));
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
        if is_key_pressed(KeyCode::V) {
            self.net_view = match self.net_view {
                NetView::Off => NetView::Split,
                NetView::Split => NetView::Full,
                NetView::Full => NetView::Off,
            };
            self.network_view.clear();
            self.seen_decisions = 0;
        }
        let toggles: [(KeyCode, &mut bool); 9] = [
            (KeyCode::O, &mut self.leaderboard.open),
            (KeyCode::D, &mut self.driver_info.open),
            (KeyCode::C, &mut self.dashboard.open),
            (KeyCode::I, &mut self.show_inputs),
            (KeyCode::E, &mut self.show_effects),
            (KeyCode::T, &mut self.show_telemetry),
            (KeyCode::H, &mut self.show_help),
            (KeyCode::M, &mut self.map_view),
            (KeyCode::P, &mut self.paused),
        ];
        for (key, flag) in toggles {
            if is_key_pressed(key) {
                *flag = !*flag;
            }
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.time_scale_index = self.time_scale_index.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.time_scale_index = (self.time_scale_index + 1).min(TIME_SCALES.len() - 1);
        }

        let (mx, my) = mouse_position();
        let over_net = self.net_view == NetView::Full || (self.net_view == NetView::Split && mx >= self.track_width());
        let over_ui = self.leaderboard.hovered() || self.driver_info.hovered() || over_net;
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 && !over_ui {
            self.camera.zoom_by(if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 });
        }
        if is_mouse_button_pressed(MouseButton::Left) && !over_ui && my < screen_height() - STRIP_H {
            let at = self.camera.screen_to_world(mx, my);
            let alpha = self.alpha();
            let nearest = self
                .racers
                .iter()
                .enumerate()
                .map(|(i, r)| (i, r.shown_state(alpha).position.distance(at)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, distance)) = nearest
                && distance < CLICK_RADIUS
            {
                self.follow(i);
            }
        }
    }

    fn track_width(&self) -> f32 {
        match self.net_view {
            NetView::Split => (screen_width() * SPLIT_TRACK_SHARE).round(),
            NetView::Off | NetView::Full => screen_width(),
        }
    }

    pub fn draw(&mut self) {
        if self.dashboard.open {
            self.dashboard.draw();
            return;
        }
        let view_w = self.track_width();
        if self.net_view != NetView::Full {
            self.draw_track_view(view_w);
        }
        if self.net_view != NetView::Off
            && let Some(r) = self.racers.get(self.followed)
        {
            let rect = match self.net_view {
                NetView::Split => Rect::new(view_w, 0.0, screen_width() - view_w, screen_height()),
                _ => Rect::new(0.0, 0.0, screen_width(), screen_height() - STRIP_H),
            };
            self.network_view.draw(r.network(), rect, true);
        }
        self.draw_status(view_w);
        if self.net_view != NetView::Full {
            let generation = self.top.as_ref().map_or(0, |t| t.header.generation);
            let colour = |r: &Racer| island_colour(r.entry.island);
            // Info last: on top where a wide leaderboard reaches it.
            let row = self.leaderboard.show(&self.racers, self.followed, generation, view_w, colour);
            let info = self.driver_info.show(&self.racers, self.followed, generation, view_w, colour);
            if let Some(i) = info.or(row) {
                self.follow(i);
            }
        }
        self.dashboard.draw();
    }

    fn draw_track_view(&mut self, view_w: f32) {
        let alpha = self.alpha();
        self.camera.apply();
        ground::draw(&self.camera);
        self.mesh.draw(&self.camera);
        let Some(followed) = self.racers.get(self.followed) else {
            set_default_camera();
            return;
        };
        self.mesh.draw_zones(&followed.episode().scorer.zone_lit);
        for r in &self.racers {
            r.skids.draw(&self.camera);
        }
        let params = &followed.episode().world.car.params;
        if self.show_effects {
            self.effects.draw_ground(&self.camera, params);
            light::pass(|| {
                self.mesh.draw_lights(&self.camera, &followed.episode().scorer.zone_lit);
                for (i, r) in self.racers.iter().enumerate() {
                    let strength = if i == self.followed { 1.0 } else if r.end.is_some() { 0.0 } else { PACK_BEAMS };
                    render::car::draw_beams(&r.shown_state(alpha), &r.episode().world.car.params, strength);
                }
            });
        }
        // The pack first, the followed car on top.
        for (i, r) in self.racers.iter().enumerate() {
            if i == self.followed {
                continue;
            }
            let fade = if r.end.is_some() { 0.3 } else { 0.85 };
            let paint = island_colour(r.entry.island);
            let state = r.shown_state(alpha);
            render::car::draw_painted(&state, &r.episode().world.car.params, &self.no_damage, paint, roof_of(paint), fade);
        }
        let state = followed.shown_state(alpha);
        let paint = island_colour(followed.entry.island);
        let p = state.position;
        draw_circle_lines(p.x as f32, p.y as f32, 3.4, 0.18, theme::TEXT);
        render::car::draw_painted(&state, params, &self.effects.damage, paint, roof_of(paint), 1.0);
        if self.show_effects {
            self.effects.draw_air();
            light::pass(|| {
                for (i, r) in self.racers.iter().enumerate() {
                    let fade = if i == self.followed { 1.0 } else if r.end.is_some() { 0.3 } else { 0.85 };
                    let damage = if i == self.followed { &self.effects.damage } else { &self.no_damage };
                    render::car::draw_tail_lights(&r.shown_state(alpha), &r.episode().world.car.params, damage, fade);
                }
                self.effects.draw_lights();
            });
        }
        if self.show_telemetry {
            telemetry::draw_world(&state, params);
        }

        set_default_camera();
        if self.show_effects {
            light::vignette(view_w, screen_height());
        }
        let bottom = screen_height() - STRIP_H;
        let world = &followed.episode().world;
        hud::draw_at(&state, params, view_w, bottom);
        hud::draw_track_info(world, view_w);
        self.score_hud.draw(&followed.episode().scorer, view_w);
        if self.show_telemetry {
            telemetry::draw_panel(&state, params, view_w);
            driver_prep_panel::draw_at(world, view_w, bottom);
        }
        if self.show_inputs
            && let (Some(observation), _) = followed.last_decision()
        {
            inputs_panel::draw(observation, &state.controls);
        }
    }

    fn draw_status(&self, w: f32) {
        let h = screen_height();
        let mut parts = vec![];
        if self.paused {
            parts.push("PAUSED".to_owned());
        }
        let scale = TIME_SCALES[self.time_scale_index];
        if scale != 1.0 {
            parts.push(format!("{scale}x"));
        }
        match &self.top {
            Some(top) => {
                let driving = self.racers.iter().filter(|r| r.end.is_none()).count();
                parts.push(format!(
                    "WATCH {} gen {} ({})   heat {}: {driving}/{} driving   on {}",
                    self.run,
                    top.header.generation,
                    top.header.stage,
                    self.heats,
                    self.racers.len(),
                    self.track.def.name
                ));
                if let Some(r) = self.racers.get(self.followed) {
                    parts.push(format!("following {}", r.label));
                }
                if let Some(next) = &self.pending {
                    parts.push(format!("next heat: gen {}", next.header.generation));
                }
            }
            None => parts.push(format!("WATCH {}: waiting for {}", self.run, self.top_path().display())),
        }
        let line = parts.join("   ");
        let natural = measure_text(&line, None, 20, 1.0).width;
        let size = (20.0 * (w - 32.0) / natural).clamp(11.0, 20.0);
        text(&line, 16.0, h - 34.0, size, theme::SODIUM);

        if let Some((message, _)) = &self.toast {
            let width = measure_text(message, None, 22, 1.0).width;
            text(message, ((w - width) / 2.0).max(16.0), 170.0, 22.0, theme::NEON_CYAN);
        }
        let help = if self.show_help {
            "F / click car or row: follow  Tab leaderboard details  G group by island  D driver info  J replays  U review grid  R restart heat  L library track  N generated track  O leaderboard  M map  V network  I inputs  E effects  T telemetry  C run charts  [ ] speed  P pause  H help"
        } else {
            "H: help"
        };
        let natural = measure_text(help, None, 15, 1.0).width;
        let size = (15.0 * (w - 32.0) / natural).clamp(9.0, 15.0);
        text(help, 16.0, h - 12.0, size, theme::TEXT_DIM);
    }
}

pub(super) fn build(choice: &TrackChoice, generator: &GeneratorParams) -> Result<Arc<Track>, String> {
    let def = match choice {
        TrackChoice::Library(path) => TrackDef::load(path).map_err(|e| format!("{}: {e}", path.display()))?,
        TrackChoice::Generated(seed) => {
            generator::generate(generator, *seed).map_err(|e| format!("generated track {seed}: {e}"))?
        }
    };
    let name = def.name.clone();
    Track::build(def).map(Arc::new).map_err(|e| format!("{name}: {e}"))
}

/// The library track after the current one (or the first).
pub(super) fn next_library_track(choice: &TrackChoice) -> Option<PathBuf> {
    let paths = library_paths();
    let next = match choice {
        TrackChoice::Library(current) => paths.iter().position(|p| p == current).map_or(0, |i| i + 1),
        TrackChoice::Generated(_) => 0,
    };
    paths.get(next % paths.len().max(1)).cloned()
}

/// Every library track file, sorted.
fn library_paths() -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(workspace_file("tracks")) else { return vec![] };
    let mut paths: Vec<PathBuf> =
        dir.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    paths.sort();
    paths
}

/// The first `tracks.fixed` entry of a run's training config, if any.
pub(super) fn first_fixed_track(training: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(training).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let first = value.get("tracks")?.get("fixed")?.as_array()?.first()?.as_str()?;
    Some(workspace_file(first))
}
