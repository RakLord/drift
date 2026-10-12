//! Replay screen: watch a recorded run, scrub through it, slow it down.
//!
//! Playback interpolates between the recorded frames. Smoke, skids and
//! sparks are made again from the recorded wheel states and contacts, by the
//! same effects code as live driving; seeking backwards rebuilds them from
//! the start. With the genome and inputs recorded, the network view replays
//! exactly what the driver saw and did.

mod picker;
mod timeline;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent::replay_file::{self, Replay};
use macroquad::prelude::*;
use nn::Network;
use sim::World;
use sim::episode::TICK;
use sim::scoring::Scorer;
use sim::sensors::observation::{INPUTS, Observation};
use sim::track::Track;

use super::play::{NetView, track_bounds};
use crate::dashboard::Dashboard;
use crate::debug::history::History;
use crate::debug::network_view::NetworkView;
use crate::debug::{driver_prep_panel, inputs_panel, telemetry};
use crate::render::score::ScoreHud;
use crate::render::{self, camera::FollowCamera, effects::Effects, ground, hud, text, theme, track::TrackMesh};
use picker::Picker;
use timeline::Timeline;

const SPEEDS: [f64; 7] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
const NORMAL_SPEED: usize = 3;
/// Arrow keys jump this far.
const SKIP_SECS: f64 = 2.0;
/// A jump longer than this doesn't replay the score popups in between.
const POPUP_JUMP_TICKS: u64 = 60;
/// Share of the window the track keeps when the network view is split.
const SPLIT_TRACK_SHARE: f32 = 0.45;
/// Room at the bottom for the status line, help and timeline; the HUD sits
/// above it.
const STRIP_H: f32 = 84.0;

/// The newest replay in a run's replays folder.
pub fn newest(run_dir: &Path) -> Option<PathBuf> {
    picker::list(&run_dir.join("replays")).into_iter().next()
}

pub struct ReplayScreen {
    path: PathBuf,
    replay: Replay,
    world: World,
    scorer: Scorer,
    mesh: TrackMesh,
    score_hud: ScoreHud,
    effects: Effects,
    /// Effects and the telemetry history are up to date through this tick.
    effects_tick: u64,
    history: History,
    camera: FollowCamera,
    network: Option<Network>,
    network_view: NetworkView,
    net_view: NetView,
    /// The decision the network view last showed.
    seen_decision: Option<usize>,
    /// Playback position, in (fractional) ticks.
    tick: f64,
    playing: bool,
    speed_index: usize,
    timeline: Timeline,
    picker: Picker,
    dashboard: Dashboard,
    /// From the last draw (timeline / picker), applied on the next update.
    seek: Option<f64>,
    open: Option<PathBuf>,
    map_view: bool,
    show_effects: bool,
    show_telemetry: bool,
    show_inputs: bool,
    show_help: bool,
    toast: Option<(String, f64)>,
    switch: Option<super::Switch>,
}

impl ReplayScreen {
    pub fn new(path: PathBuf, picker_open: bool, charts: bool) -> Result<Self, String> {
        let replay = replay_file::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let h = &replay.header;
        let track = Track::build(h.track.clone()).map_err(|e| format!("{}: track doesn't build: {e}", path.display()))?;
        let mesh = TrackMesh::new(&track);
        let scorer = Scorer::new(h.scoring.clone(), &track);
        let world = World::new(Arc::new(track), h.car.clone());
        let network = replay.genome.as_ref().map(|g| Network::new(g.header.spec.clone(), g.params.clone()));
        // runs/<run>/replays/x.replay -> runs/<run>/metrics.csv
        let metrics = path.parent().and_then(Path::parent).map(|run| run.join("metrics.csv")).filter(|p| p.exists());
        let toast = replay.dropped.clone().map(|why| (why, 6.0));
        let start = replay.recording.frames.first().map_or(world.car.state.position, |f| f.position);
        let mut picker = Picker::new(picker_open);
        picker.refresh(&path);
        let mut screen = Self {
            path,
            replay,
            world,
            scorer,
            mesh,
            score_hud: ScoreHud::new(),
            effects: Effects::new(),
            effects_tick: 0,
            history: History::new(),
            camera: FollowCamera::new(start),
            network,
            network_view: NetworkView::new(),
            net_view: NetView::Off,
            seen_decision: None,
            tick: 0.0,
            playing: true,
            speed_index: NORMAL_SPEED,
            timeline: Timeline::new(),
            picker,
            dashboard: Dashboard::new(metrics, charts),
            seek: None,
            open: None,
            map_view: false,
            show_effects: true,
            show_telemetry: true,
            show_inputs: false,
            show_help: true,
            toast,
            switch: None,
        };
        screen.show_frame();
        Ok(screen)
    }

    /// Another view asked for (Q: this run's top cars), if any.
    pub fn take_switch(&mut self) -> Option<super::Switch> {
        self.switch.take()
    }

    pub fn notify(&mut self, message: String) {
        self.toast = Some((message, 5.0));
    }

    /// Start with the network view split beside the track (`--net`).
    pub fn open_network(&mut self) {
        if self.network.is_some() {
            self.net_view = NetView::Split;
        }
    }

    /// Open another replay, keeping how things are shown.
    fn switch_to(&mut self, path: PathBuf) {
        match Self::new(path, self.picker.open, self.dashboard.open) {
            Ok(mut next) => {
                next.map_view = self.map_view;
                next.show_effects = self.show_effects;
                next.show_telemetry = self.show_telemetry;
                next.show_inputs = self.show_inputs;
                next.show_help = self.show_help;
                next.speed_index = self.speed_index;
                next.net_view = if next.network.is_some() { self.net_view } else { NetView::Off };
                next.toast = next.toast.or_else(|| Some((next.replay.header.title.clone(), 3.0)));
                *self = next;
            }
            Err(err) => self.toast = Some((err, 6.0)),
        }
    }

    fn end_tick(&self) -> f64 {
        self.replay.recording.ticks as f64
    }

    pub fn update(&mut self, dt: f64) {
        let dt = dt.min(0.1);
        if let Some(path) = self.open.take() {
            self.switch_to(path);
            return;
        }
        self.handle_keys();
        self.dashboard.update(dt);
        self.score_hud.update(dt);
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }

        let before = self.tick;
        if let Some(tick) = self.seek.take() {
            self.tick = tick;
        } else if self.playing {
            self.tick += dt * SPEEDS[self.speed_index] / TICK;
        }
        if self.tick >= self.end_tick() {
            self.tick = self.end_tick();
            self.playing = false;
        }
        self.tick = self.tick.max(0.0);

        let (from, to) = (before.floor() as u64, self.tick.floor() as u64);
        if to > from && to - from <= POPUP_JUMP_TICKS {
            let events: Vec<_> = self.replay.recording.events_between(from, to).iter().map(|e| e.event).collect();
            self.score_hud.on_events(&events);
        } else if to != from {
            self.score_hud.clear();
        }
        self.catch_up_effects();
        self.show_frame();

        self.camera.set_view_width(self.track_width());
        if self.map_view {
            let (min, max) = track_bounds(&self.world.track);
            self.camera.show_area(min, max);
        } else if (self.tick - before).abs() > 30.0 {
            self.camera.snap_to(self.world.car.state.position);
        } else {
            self.camera.update(self.world.car.state.position, self.world.car.state.velocity, dt);
        }
    }

    /// Bring smoke, skids, sparks and the telemetry history up to the
    /// playhead, from the start if it moved backwards.
    fn catch_up_effects(&mut self) {
        let target = self.tick.floor() as u64;
        if target < self.effects_tick {
            self.effects.clear();
            self.history.clear();
            self.effects_tick = 0;
        }
        let rec = &self.replay.recording;
        let params = &self.world.car.params;
        for t in self.effects_tick + 1..=target {
            let state = rec.frame_at(t as f64).car_state();
            let contacts: Vec<_> = rec.contacts_between(t - 1, t).iter().map(|c| c.contact).collect();
            self.effects.on_tick(&state, params, &contacts, TICK);
            self.history.record(&state);
        }
        self.effects_tick = target;
    }

    /// Put the playhead's frame into the world, scorer and network view.
    fn show_frame(&mut self) {
        let rec = &self.replay.recording;
        let frame = rec.frame_at(self.tick);
        self.world.set_car_state(frame.car_state());
        self.world.progress = frame.progress;
        frame.apply_score(&mut self.scorer);
        let tick = self.tick.floor() as u64;
        self.scorer.zone_lit = rec.zones_lit_at(tick, self.scorer.zone_lit.len());

        let decision = rec.decision_at(tick);
        let index = decision.map(|(i, _)| i);
        if index != self.seen_decision {
            if index < self.seen_decision {
                // Went backwards: the view's history no longer applies.
                self.network_view.clear();
            }
            if let (Some(net), Some((_, d))) = (&self.network, decision) {
                self.network_view.on_decision(net, &d.inputs);
            }
            self.seen_decision = index;
        }
    }

    fn handle_keys(&mut self) {
        if is_key_pressed(KeyCode::Space) {
            if self.tick >= self.end_tick() {
                self.seek = Some(0.0);
            }
            self.playing = !self.playing || self.seek.is_some();
        }
        let skip = SKIP_SECS / TICK;
        if is_key_pressed(KeyCode::Left) {
            self.seek = Some((self.tick - skip).max(0.0));
        }
        if is_key_pressed(KeyCode::Right) {
            self.seek = Some((self.tick + skip).min(self.end_tick()));
        }
        let frame = f64::from(self.replay.recording.frame_every.max(1));
        if is_key_pressed(KeyCode::Comma) {
            self.playing = false;
            self.seek = Some(((self.tick / frame).ceil() - 1.0).max(0.0) * frame);
        }
        if is_key_pressed(KeyCode::Period) {
            self.playing = false;
            self.seek = Some((((self.tick / frame).floor() + 1.0) * frame).min(self.end_tick()));
        }
        if is_key_pressed(KeyCode::Q) {
            let run = Some(self.replay.header.run.clone()).filter(|r| !r.is_empty()).or_else(|| self.path.parent().and_then(super::run_of));
            match run {
                Some(run) => self.switch = Some(super::Switch::Watch(run)),
                None => self.toast = Some(("not from a training run".to_owned(), 3.0)),
            }
        }
        if is_key_pressed(KeyCode::Home) {
            self.seek = Some(0.0);
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            self.speed_index = self.speed_index.saturating_sub(1);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.speed_index = (self.speed_index + 1).min(SPEEDS.len() - 1);
        }
        for (key, step) in [(KeyCode::Up, -1), (KeyCode::Down, 1)] {
            if is_key_pressed(key) {
                match self.picker.neighbour(&self.path, step) {
                    Some(path) => self.open = Some(path),
                    None => self.toast = Some(("no more replays that way".to_owned(), 2.0)),
                }
            }
        }
        if is_key_pressed(KeyCode::V) {
            if self.network.is_some() {
                self.net_view = match self.net_view {
                    NetView::Off => NetView::Split,
                    NetView::Split => NetView::Full,
                    NetView::Full => NetView::Off,
                };
            } else {
                let why = self.replay.dropped.clone().unwrap_or_else(|| "no network in this replay".to_owned());
                self.toast = Some((why, 3.0));
            }
        }
        let toggles: [(KeyCode, &mut bool); 8] = [
            (KeyCode::O, &mut self.picker.open),
            (KeyCode::C, &mut self.dashboard.open),
            (KeyCode::I, &mut self.show_inputs),
            (KeyCode::E, &mut self.show_effects),
            (KeyCode::T, &mut self.show_telemetry),
            (KeyCode::H, &mut self.show_help),
            (KeyCode::M, &mut self.map_view),
            (KeyCode::P, &mut self.playing),
        ];
        for (key, flag) in toggles {
            if is_key_pressed(key) {
                *flag = !*flag;
            }
        }
        if is_key_pressed(KeyCode::O) && self.picker.open {
            self.picker.refresh(&self.path);
        }
        let (_, wheel) = mouse_wheel();
        let over_net = self.net_view == NetView::Full
            || (self.net_view == NetView::Split && mouse_position().0 >= self.track_width());
        if wheel != 0.0 && !self.picker.hovered() && !over_net {
            self.camera.zoom_by(if wheel > 0.0 { 1.1 } else { 1.0 / 1.1 });
        }
    }

    fn track_width(&self) -> f32 {
        match self.net_view {
            NetView::Split => (screen_width() * SPLIT_TRACK_SHARE).round(),
            NetView::Off | NetView::Full => screen_width(),
        }
    }

    pub fn draw(&mut self) {
        let view_w = self.track_width();
        if self.net_view != NetView::Full {
            self.draw_track_view(view_w);
        }
        self.draw_network_view();
        self.draw_status(view_w);
        if let Some(tick) = self.timeline.show(&self.replay.recording, self.tick, view_w) {
            self.seek = Some(tick);
        }
        if let Some(path) = self.picker.show(&self.path) {
            self.open = Some(path);
        }
        self.dashboard.draw();
    }

    fn draw_track_view(&mut self, view_w: f32) {
        let car = &self.world.car;
        let state = &car.state;
        self.camera.apply();
        ground::draw(&self.camera);
        self.mesh.draw(&self.camera);
        self.mesh.draw_zones(&self.scorer.zone_lit);
        if self.show_effects {
            self.effects.draw_ground(&self.camera, &car.params);
        }
        render::car::draw(state, &car.params, &self.effects.damage);
        if self.show_effects {
            self.effects.draw_air();
        }
        if self.show_telemetry {
            telemetry::draw_world(state, &car.params);
        }

        set_default_camera();
        let bottom = screen_height() - STRIP_H;
        hud::draw_at(state, &car.params, view_w, bottom);
        hud::draw_track_info(&self.world, view_w);
        self.score_hud.draw(&self.scorer, view_w);
        if self.show_telemetry {
            if self.net_view == NetView::Off {
                telemetry::draw_panel(state, &car.params, view_w);
                // The picker sits where the history graph is.
                if !self.picker.open {
                    self.history.draw();
                }
            }
            driver_prep_panel::draw_at(&self.world, view_w, bottom);
        }
        if self.show_inputs
            && self.net_view == NetView::Off
            && let Some((_, d)) = self.replay.recording.decision_at(self.tick.floor() as u64)
            && let Ok(values) = <[f32; INPUTS]>::try_from(d.inputs.as_slice())
        {
            inputs_panel::draw(&Observation { values }, &state.controls);
        }
    }

    fn draw_network_view(&mut self) {
        let Some(net) = &self.network else { return };
        let rect = match self.net_view {
            NetView::Off => return,
            NetView::Split => {
                let x = self.track_width();
                Rect::new(x, 0.0, screen_width() - x, screen_height())
            }
            NetView::Full => Rect::new(0.0, 0.0, screen_width(), screen_height() - STRIP_H),
        };
        self.network_view.draw(net, rect, true);
    }

    fn draw_status(&self, w: f32) {
        let h = screen_height();
        let header = &self.replay.header;
        let speed = SPEEDS[self.speed_index];
        let state = if self.playing { format!("REPLAY {speed}x") } else { "REPLAY PAUSED".to_owned() };
        let end = format!("{}  {:.0} pts  {:.2} laps", header.result.end.name(), header.result.score.total(), header.result.laps());
        let time = format!("{:.1} / {:.1} s", self.tick * TICK, self.end_tick() * TICK);
        let line = format!("{state}   {time}   {}   ({end})", header.title);
        let natural = measure_text(&line, None, 20, 1.0).width;
        let size = (20.0 * (w - 32.0) / natural).clamp(11.0, 20.0);
        text(&line, 16.0, h - 64.0, size, theme::SODIUM);

        if let Some((message, _)) = &self.toast {
            let width = measure_text(message, None, 22, 1.0).width;
            text(message, ((w - width) / 2.0).max(16.0), 170.0, 22.0, theme::NEON_CYAN);
        }

        let help: &[&str] = if self.show_help {
            &[
                "Space play/pause  Left/Right -/+2 s  , . frame  Home restart  [ ] speed  timeline: click/drag",
                "O replays  Up/Down prev/next  Q top cars  V network  I inputs  M map  E effects  T telemetry  C run charts  H help",
            ]
        } else {
            &["H: help"]
        };
        let widest = help.iter().map(|l| measure_text(l, None, 15, 1.0).width).fold(0.0, f32::max);
        let size = (15.0 * (w - 32.0) / widest).clamp(9.0, 15.0);
        for (i, line) in help.iter().rev().enumerate() {
            text(line, 16.0, h - 28.0 - i as f32 * (size + 2.0), size, theme::TEXT_DIM);
        }
    }
}
