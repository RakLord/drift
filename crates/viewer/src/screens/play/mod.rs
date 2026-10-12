//! Free-drive screen: one car on a track, driven by you.

mod configs;
mod generator_panel;
mod tracks;

use std::sync::Arc;

use common::Vec2;
use common::math::{angle_diff, wrap_angle};
use macroquad::prelude::*;
use sim::World;
use sim::car::CarState;
use sim::scoring::Scorer;
use sim::track::generator::GeneratorParams;
use sim::track::{Track, TrackDef};

use crate::app::Options;
use crate::debug::{driver_prep_panel, gamepad_panel, history::History, telemetry};
use crate::input::{Input, autopilot::Autopilot};
use crate::render::score::ScoreHud;
use crate::render::{self, camera::FollowCamera, effects::Effects, ground, hud, text, theme, track::TrackMesh};
use configs::{Configs, Reload};
use generator_panel::{GeneratorPanel, PanelAction};
use tracks::TrackSource;

const TICK: f64 = 1.0 / 120.0;
/// Never simulate more than this many ticks per frame (avoids a death spiral
/// after a stall, e.g. dragging the window).
const MAX_TICKS_PER_FRAME: u32 = 12;
const TIME_SCALES: [f64; 5] = [0.1, 0.25, 0.5, 1.0, 2.0];

pub struct PlayScreen {
    world: World,
    source: TrackSource,
    /// Definition of the current track (for keeping generated ones).
    def: TrackDef,
    /// Generator settings: from config/generator.toml, editable in the panel.
    gen_params: GeneratorParams,
    /// Seed shown in the generator panel.
    gen_seed: u64,
    panel: GeneratorPanel,
    /// Clicked in the panel during the last draw; applied on the next update.
    panel_action: Option<PanelAction>,
    mesh: TrackMesh,
    scorer: Scorer,
    score_hud: ScoreHud,
    previous: Pose,
    configs: Configs,
    input: Input,
    autopilot: Option<Autopilot>,
    camera: FollowCamera,
    history: History,
    effects: Effects,
    accumulator: f64,
    time_scale_index: usize,
    paused: bool,
    map_view: bool,
    show_effects: bool,
    show_telemetry: bool,
    show_gamepad: bool,
    show_help: bool,
    toast: Option<(String, f64)>,
}

#[derive(Clone, Copy)]
struct Pose {
    position: Vec2,
    heading: f64,
}

impl Pose {
    fn of(state: &CarState) -> Self {
        Self { position: state.position, heading: state.heading }
    }
}

impl PlayScreen {
    pub fn new(options: &Options) -> Self {
        let mut warnings = vec![];
        let configs = Configs::load(&mut warnings);
        let gen_params = configs.generator();
        let mut source = options.seed.map_or(TrackSource::Library, TrackSource::Generated);
        let (track, def) = tracks::build(source, configs.library_track(), &gen_params).unwrap_or_else(|err| {
            warnings.extend(err);
            source = TrackSource::Library;
            tracks::build(source, configs.library_track(), &gen_params).expect("library track must build")
        });

        let mesh = TrackMesh::new(&track);
        let scorer = Scorer::new(configs.scoring(), &track);
        let world = World::new(Arc::new(track), configs.car());
        let start = world.car.state.position;
        Self {
            previous: Pose::of(&world.car.state),
            world,
            source,
            def,
            gen_params,
            gen_seed: options.seed.unwrap_or(1),
            panel: GeneratorPanel::new(options.generator),
            panel_action: None,
            mesh,
            scorer,
            score_hud: ScoreHud::new(),
            configs,
            input: Input::new(),
            autopilot: options.autopilot.then(Autopilot::new),
            camera: FollowCamera::new(start),
            history: History::new(),
            effects: Effects::new(),
            accumulator: 0.0,
            time_scale_index: 3,
            paused: false,
            map_view: options.map_view,
            show_effects: true,
            show_telemetry: true,
            show_gamepad: false,
            show_help: true,
            toast: (!warnings.is_empty()).then(|| (warnings.join("; "), 8.0)),
        }
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        self.handle_keys();
        self.apply_panel_action();
        self.apply_reloads(dt);
        self.score_hud.update(dt);
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }

        // Read input every frame (the gamepad needs polling) even if paused.
        let input = self.input.controls(dt);
        if self.input.reset_pressed() {
            self.reset();
        }
        if self.paused {
            return;
        }

        self.accumulator += dt * TIME_SCALES[self.time_scale_index];
        let mut ticks = 0;
        while self.accumulator >= TICK && ticks < MAX_TICKS_PER_FRAME {
            let controls = match &mut self.autopilot {
                Some(autopilot) => autopilot.controls(&self.world, TICK),
                None => input,
            };
            self.previous = Pose::of(&self.world.car.state);
            self.world.step(controls, TICK);
            let car = &self.world.car;
            self.effects.on_tick(&car.state, &car.params, self.world.contacts(), TICK);
            let events = self.scorer.update(&self.world, self.world.contacts(), TICK);
            self.score_hud.on_events(events);
            self.history.record(&car.state);
            self.accumulator -= TICK;
            ticks += 1;
        }
        if ticks == MAX_TICKS_PER_FRAME {
            self.accumulator = 0.0;
        }

        if self.map_view {
            let (min, max) = track_bounds(&self.world.track);
            self.camera.show_area(min, max);
        } else {
            let shown = self.interpolated();
            self.camera.update(shown.position, self.world.car.state.velocity, dt);
        }
    }

    pub fn draw(&mut self) {
        let shown = self.interpolated();
        let car = &self.world.car;
        let mut state = car.state.clone();
        state.position = shown.position;
        state.heading = shown.heading;

        self.camera.apply();
        ground::draw(&self.camera);
        self.mesh.draw(&self.camera);
        self.mesh.draw_zones(&self.scorer.zone_lit);
        if self.show_effects {
            self.effects.draw_ground(&self.camera, &car.params);
        }
        render::car::draw(&state, &car.params, &self.effects.damage);
        if self.show_effects {
            self.effects.draw_air();
        }
        if self.show_telemetry {
            telemetry::draw_world(&state, &car.params);
        }

        set_default_camera();
        hud::draw(&state, &car.params);
        hud::draw_track_info(&self.world);
        self.score_hud.draw(&self.scorer);
        if self.show_telemetry {
            telemetry::draw_panel(&state, &car.params);
            self.history.draw();
            driver_prep_panel::draw(&self.world);
        }
        if self.show_gamepad {
            gamepad_panel::draw(&self.input.gamepad);
        }
        self.draw_status();
        // Last, so it sits on top. It handles its own input as it draws.
        if let Some(action) = self.panel.show(&mut self.gen_params, &mut self.gen_seed) {
            self.panel_action = Some(action);
        }
    }

    /// Render pose between the last two ticks, so motion is smooth at any
    /// refresh rate.
    fn interpolated(&self) -> Pose {
        let alpha = (self.accumulator / TICK).clamp(0.0, 1.0);
        let current = Pose::of(&self.world.car.state);
        Pose {
            position: self.previous.position.lerp(current.position, alpha),
            heading: wrap_angle(self.previous.heading + angle_diff(self.previous.heading, current.heading) * alpha),
        }
    }

    fn reset(&mut self) {
        self.world.reset();
        self.previous = Pose::of(&self.world.car.state);
        self.camera.snap_to(self.world.car.state.position);
        self.history.clear();
        self.effects.clear();
        self.scorer = Scorer::new(self.scorer.params.clone(), &self.world.track);
        self.score_hud.clear();
        if self.autopilot.is_some() {
            self.autopilot = Some(Autopilot::new());
        }
    }

    /// Switch to the track for `source`, keeping the current one on failure.
    fn switch_track(&mut self, source: TrackSource) {
        match tracks::build(source, self.configs.library_track(), &self.gen_params) {
            Ok((track, def)) => {
                let name = track.def.name.clone();
                self.install_track(track, def);
                self.source = source;
                if let TrackSource::Generated(seed) = source {
                    self.gen_seed = seed;
                }
                self.panel.problem.clear();
                self.toast = Some((name, 2.0));
            }
            Err(lines) => {
                if matches!(source, TrackSource::Generated(_)) {
                    self.panel.problem = lines.clone();
                }
                self.toast = Some((lines.join("\n"), 10.0));
            }
        }
    }

    fn install_track(&mut self, track: Track, def: TrackDef) {
        self.mesh = TrackMesh::new(&track);
        self.world = World::new(Arc::new(track), self.world.car.params.clone());
        self.def = def;
        self.reset();
    }

    fn handle_keys(&mut self) {
        let toggles: [(KeyCode, &mut bool); 7] = [
            (KeyCode::F, &mut self.panel.open),
            (KeyCode::P, &mut self.paused),
            (KeyCode::E, &mut self.show_effects),
            (KeyCode::T, &mut self.show_telemetry),
            (KeyCode::G, &mut self.show_gamepad),
            (KeyCode::H, &mut self.show_help),
            (KeyCode::M, &mut self.map_view),
        ];
        for (key, flag) in toggles {
            if is_key_pressed(key) {
                *flag = !*flag;
            }
        }
        if is_key_pressed(KeyCode::R) {
            self.reset();
        }
        if is_key_pressed(KeyCode::N) {
            let next = match self.source {
                TrackSource::Generated(seed) => seed + 1,
                TrackSource::Library => 1,
            };
            self.switch_track(TrackSource::Generated(next));
        }
        if is_key_pressed(KeyCode::L) {
            self.switch_track(TrackSource::Library);
        }
        if is_key_pressed(KeyCode::K) {
            self.keep_track();
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.time_scale_index = self.time_scale_index.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.time_scale_index = (self.time_scale_index + 1).min(TIME_SCALES.len() - 1);
        }
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 && !self.panel.hovered() {
            self.camera.zoom_by(if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 });
        }
    }

    fn apply_panel_action(&mut self) {
        match self.panel_action.take() {
            Some(PanelAction::Generate) => self.switch_track(TrackSource::Generated(self.gen_seed)),
            Some(PanelAction::Keep) => self.keep_track(),
            Some(PanelAction::SaveDefaults) => {
                self.toast = Some(match generator_panel::save_defaults(&self.gen_params) {
                    Ok(path) => (format!("saved generator settings to {path}"), 4.0),
                    Err(err) => (format!("couldn't save settings: {err}"), 6.0),
                });
            }
            None => {}
        }
    }

    fn keep_track(&mut self) {
        let TrackSource::Generated(seed) = self.source else {
            self.toast = Some(("already a library track".to_owned(), 2.0));
            return;
        };
        self.toast = Some(match tracks::keep(&self.def, seed) {
            Ok(path) => (format!("kept as {}", path.display()), 4.0),
            Err(err) => (format!("couldn't keep track: {err}"), 6.0),
        });
    }

    fn apply_reloads(&mut self, dt: f64) {
        for reload in self.configs.poll(dt) {
            match reload {
                Reload::Car(params) => {
                    self.world.car.params = params;
                    self.toast = Some(("car.toml reloaded".to_owned(), 2.0));
                }
                Reload::Scoring(params) => {
                    self.scorer.params = params;
                    self.toast = Some(("scoring.toml reloaded".to_owned(), 2.0));
                }
                Reload::LibraryTrack if self.source == TrackSource::Library => {
                    self.switch_track(TrackSource::Library);
                }
                Reload::Generator => {
                    self.gen_params = self.configs.generator();
                    if let TrackSource::Generated(_) = self.source {
                        self.switch_track(self.source);
                    }
                }
                Reload::LibraryTrack => {}
                Reload::Rejected(message) => self.toast = Some((message, 6.0)),
            }
        }
    }

    fn draw_status(&self) {
        let (w, h) = (screen_width(), screen_height());
        let scale = TIME_SCALES[self.time_scale_index];
        let mut status = vec![];
        if self.paused {
            status.push("PAUSED".to_owned());
        }
        if scale != 1.0 {
            status.push(format!("{scale}x speed"));
        }
        if self.autopilot.is_some() {
            status.push("AUTOPILOT".to_owned());
        }
        if !status.is_empty() {
            text(&status.join("   "), w / 2.0 - 80.0, h - 70.0, 28.0, theme::SODIUM);
        }
        if let Some((message, _)) = &self.toast {
            // Multi-line toasts stack upwards from just above the status line.
            for (i, line) in message.lines().rev().enumerate() {
                let width = measure_text(line, None, 22, 1.0).width;
                text(line, (w - width) / 2.0, h - 100.0 - i as f32 * 26.0, 22.0, theme::NEON_CYAN);
            }
        }

        let help: &[&str] = if self.show_help {
            &[
                "pad: stick steer  RT throttle  LT brake  A/RB handbrake  X/LB clutch  Start reset",
                "keys: WASD/arrows  Space handbrake  Shift clutch  R reset  P pause  [ ] speed  wheel zoom",
                "N new track  F generator  L library track  K keep track  M map  E effects  T telemetry  G gamepad  H help",
            ]
        } else {
            &["H: help"]
        };
        for (i, line) in help.iter().rev().enumerate() {
            text(line, 16.0, h - 8.0 - i as f32 * 18.0, 16.0, theme::TEXT_DIM);
        }
    }
}

/// Bounding box of the track including verges.
fn track_bounds(track: &Track) -> (Vec2, Vec2) {
    let pad = Vec2::new(1.0, 1.0) * (track.half_width() + track.def.verge + 10.0);
    let (lo, hi) = track.samples.iter().fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(lo, hi), s| {
        let p = s.position;
        (Vec2::new(lo.x.min(p.x), lo.y.min(p.y)), Vec2::new(hi.x.max(p.x), hi.y.max(p.y)))
    });
    (lo - pad, hi + pad)
}
