//! Track screen (`O` / `--tracks`): every library track as a map card, and
//! what a training set does with each (fixed, pool or off). Pick the
//! training set at the top: the defaults for new runs, a run's own config
//! (used when it resumes), or the launcher's pick (`--set`). With a run
//! picked, a strip above the library shows every track its saved driver
//! drove in its newest generation, generated ones included, and the Results
//! tab (`Tab`) shows how it did on each track over its last N generations.

mod generation;
mod results;
mod run_log;
mod training_set;

use std::path::PathBuf;

use macroquad::prelude::*;
use sim::track::TrackStats;

use super::Switch;
use super::library::{self, Entry};
use crate::dashboard::chart::{Chart, Kind, Series};
use crate::render::thumb::{Thumb, ThumbStyle};
use crate::render::{text, theme};
use crate::ui::{self, ButtonStyle, Panel};
use generation::{Drawn, LastGeneration};
use results::{RunResults, TrackResult};
pub use training_set::Source;
use training_set::{Role, TrainingSet};

/// Where the card grid and the detail pane start, below the top bar.
const TOP: f32 = 104.0;
const PANE_W: f32 = 440.0;
const CARD_W: f32 = 232.0;
const CARD_MAP_H: f32 = 140.0;
const CARD_H: f32 = CARD_MAP_H + 68.0;
const GAP: f32 = 12.0;
/// Cards in the "this generation" strip.
const SMALL_W: f32 = 168.0;
const SMALL_MAP_H: f32 = 92.0;
const SMALL_H: f32 = SMALL_MAP_H + 50.0;
/// A section title in the grid ("GENERATION 12", "LIBRARY").
const SECTION_H: f32 = 30.0;
const SCROLL_STEP: f32 = 60.0;
/// How often to check whether the run being edited is training, seconds.
const TRAINING_CHECK: f64 = 2.0;
/// The laps-per-generation chart in the Results detail pane.
const SPARK_H: f32 = 150.0;

struct Card {
    entry: Entry,
    thumb: Option<Thumb>,
    stats: Option<TrackStats>,
}

impl Card {
    fn new(entry: Entry) -> Self {
        let thumb = entry.track.as_ref().ok().map(|t| Thumb::new(t));
        let stats = entry.track.as_ref().ok().map(|t| TrackStats::of(t));
        Self { entry, thumb, stats }
    }

    fn face(&self, role: Role) -> CardFace<'_> {
        let e = &self.entry;
        CardFace {
            name: e.name.clone(),
            detail: match &self.stats {
                Some(s) => format!("{:.2} km  ·  {} corners  ·  {}", s.length / 1000.0, s.corners, e.file_name()),
                None => e.file_name(),
            },
            thumb: self.thumb.as_ref(),
            role,
            test: e.test,
        }
    }
}

/// What a full-size card shows.
struct CardFace<'a> {
    name: String,
    /// The line under the name.
    detail: String,
    thumb: Option<&'a Thumb>,
    role: Role,
    test: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Library,
    /// A run's per-track results; only with a run's training set picked.
    Results,
}

enum Action {
    Select(usize),
    /// A track in the "this generation" strip.
    SelectDrawn(usize),
    /// A library track on the Results tab.
    SelectResult(usize),
    /// A worst generated track on the Results tab.
    SelectWorst(usize),
    SwitchTab,
    Window(i32),
    /// Save the selected generated track into the library.
    Keep,
    Drive,
    SetRole(Role),
    ToggleTest,
    Rename(String),
    Delete,
    CycleSet(i32),
    Pooled(i32),
    Generated(i32),
}

pub struct TracksScreen {
    /// Showing just the launcher's pick: Esc closes the viewer.
    pub locked: bool,
    tab: Tab,
    cards: Vec<Card>,
    selected: usize,
    /// The run's newest generation (with a run's training set picked).
    last: Option<LastGeneration>,
    /// A generated track in the strip is selected instead of `selected`.
    drawn_selected: Option<usize>,
    /// Where the library grid starts in the scrolled content (below the strip).
    library_top: f32,
    /// The run's results, loaded when the Results tab first opens.
    results: Option<RunResults>,
    /// A worst generated track on the Results tab is selected instead of
    /// `selected`.
    worst_selected: Option<usize>,
    /// Training sets to pick from (just the one with `--set`).
    sources: Vec<Source>,
    set: Result<TrainingSet, String>,
    /// Whether the run being edited is training right now.
    training: bool,
    training_checked: f64,
    /// Grid columns as last drawn (for arrow keys).
    cols: usize,
    scroll: f32,
    panel: Panel,
    /// Name being typed, while renaming.
    renaming: Option<String>,
    confirm_delete: bool,
    toast: Option<(String, f64)>,
    switch: Option<Switch>,
    /// Library files deleted here, for the play screen to let go of.
    pub deleted: Vec<PathBuf>,
}

impl TracksScreen {
    /// `source`: the training set to show first. `locked`: only that one
    /// (the launcher's pick).
    pub fn new(source: Source, locked: bool) -> Self {
        let mut sources = if locked { vec![] } else { training_set::sources() };
        if !sources.contains(&source) {
            sources.insert(0, source.clone());
        }
        let mut screen = Self {
            locked,
            tab: Tab::Library,
            cards: vec![],
            selected: 0,
            last: source.run().map(LastGeneration::new),
            drawn_selected: None,
            library_top: 0.0,
            results: None,
            worst_selected: None,
            sources,
            set: TrainingSet::load(source),
            training: false,
            training_checked: f64::MAX,
            cols: 1,
            scroll: 0.0,
            panel: Panel::new(0.0, 0.0, PANE_W),
            renaming: None,
            confirm_delete: false,
            toast: None,
            switch: None,
            deleted: vec![],
        };
        screen.rescan();
        screen
    }

    /// Re-read the library, keeping the selection on the same file.
    fn rescan(&mut self) {
        let keep = self.cards.get(self.selected).map(|c| c.entry.path.clone());
        self.cards = library::scan().into_iter().map(Card::new).collect();
        self.selected = keep.and_then(|k| self.cards.iter().position(|c| c.entry.path == k)).unwrap_or(0);
    }

    pub fn take_switch(&mut self) -> Option<Switch> {
        self.switch.take()
    }

    pub fn notify(&mut self, message: String) {
        self.toast = Some((message, 5.0));
    }

    /// Whether keys are going to a text field (so Esc mustn't leave).
    pub fn captures_keyboard(&self) -> bool {
        self.renaming.is_some()
    }

    pub fn update(&mut self, dt: f64) {
        if let Some((_, remaining)) = &mut self.toast {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.toast = None;
            }
        }
        self.training_checked += dt;
        if self.training_checked >= TRAINING_CHECK {
            self.training_checked = 0.0;
            self.training = self.set.as_ref().ok().and_then(|s| s.source.run()).is_some_and(common::procs::is_training);
        }
        if let Some(last) = &mut self.last
            && last.poll(dt)
            && self.drawn_selected.is_some()
        {
            // New generation, new tracks.
            self.drawn_selected = None;
        }
        if self.tab == Tab::Results
            && let Some(results) = &mut self.results
            && results.poll(dt)
        {
            self.worst_selected = None;
        }
        if self.renaming.is_none() {
            self.handle_keys();
        }
    }

    fn handle_keys(&mut self) {
        if is_key_pressed(KeyCode::Tab) {
            self.apply(Action::SwitchTab);
        }
        let n = self.cards.len();
        if n > 0 && self.tab == Tab::Library {
            let cols = self.cols.max(1);
            let moved = if is_key_pressed(KeyCode::Right) {
                Some((self.selected + 1).min(n - 1))
            } else if is_key_pressed(KeyCode::Left) {
                Some(self.selected.saturating_sub(1))
            } else if is_key_pressed(KeyCode::Down) {
                Some((self.selected + cols).min(n - 1))
            } else if is_key_pressed(KeyCode::Up) {
                Some(self.selected.saturating_sub(cols))
            } else {
                None
            };
            if let Some(i) = moved {
                self.apply(Action::Select(i));
                self.scroll_to_selected();
            }
        }
        let role = self.selected_role();
        let action = if self.selected_generated().is_some() {
            // A generated track: all it can do is be kept (K).
            is_key_pressed(KeyCode::K).then_some(Action::Keep)
        } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            Some(Action::Drive)
        } else if is_key_pressed(KeyCode::F) {
            Some(Action::SetRole(if role == Some(Role::Fixed) { Role::Off } else { Role::Fixed }))
        } else if is_key_pressed(KeyCode::P) {
            Some(Action::SetRole(if role == Some(Role::Pool) { Role::Off } else { Role::Pool }))
        } else if is_key_pressed(KeyCode::T) && self.tab == Tab::Library {
            Some(Action::ToggleTest)
        } else if is_key_pressed(KeyCode::LeftBracket) {
            Some(Action::CycleSet(-1))
        } else if is_key_pressed(KeyCode::RightBracket) {
            Some(Action::CycleSet(1))
        } else {
            None
        };
        if let Some(action) = action {
            self.apply(action);
        }
    }

    fn selected_role(&self) -> Option<Role> {
        let card = self.cards.get(self.selected)?;
        Some(self.set.as_ref().ok()?.role(&card.entry.config_path()))
    }

    fn scroll_to_selected(&mut self) {
        let row = (self.selected / self.cols.max(1)) as f32;
        let top = self.library_top + row * (CARD_H + GAP);
        let view = screen_height() - TOP - 30.0;
        self.scroll = self.scroll.clamp(top + CARD_H - view, top);
    }

    fn apply(&mut self, action: Action) {
        if !matches!(action, Action::Delete) {
            self.confirm_delete = false;
        }
        let card = self.cards.get(self.selected);
        let result: Result<Option<String>, String> = match action {
            Action::Select(i) => {
                if i != self.selected || self.drawn_selected.is_some() {
                    self.renaming = None;
                }
                self.selected = i;
                self.drawn_selected = None;
                Ok(None)
            }
            Action::SelectResult(i) => {
                let id = self.results.as_ref().and_then(|r| r.library.get(i)).map(|t| t.summary.id.clone());
                match id.and_then(|id| self.cards.iter().position(|c| c.entry.config_path() == id)) {
                    Some(card) => {
                        (self.selected, self.worst_selected) = (card, None);
                        Ok(None)
                    }
                    None => Err("that track isn't in the library any more".into()),
                }
            }
            Action::SelectWorst(i) => {
                self.worst_selected = Some(i);
                Ok(None)
            }
            Action::SwitchTab => match self.tab {
                Tab::Results => {
                    self.tab = Tab::Library;
                    self.scroll = 0.0;
                    Ok(None)
                }
                Tab::Library => match self.set.as_ref().ok().and_then(|s| s.source.run()) {
                    Some(run) => {
                        if self.results.as_ref().is_none_or(|r| r.run != run) {
                            self.results = Some(RunResults::new(run));
                            self.worst_selected = None;
                        }
                        self.tab = Tab::Results;
                        self.scroll = 0.0;
                        Ok(None)
                    }
                    None => Err("results are per run: pick a run's training set ([ ])".into()),
                },
            },
            Action::Window(step) => {
                if let Some(results) = &mut self.results
                    && results.step_window(step)
                {
                    self.worst_selected = None;
                }
                Ok(None)
            }
            Action::SelectDrawn(i) => {
                self.renaming = None;
                let id = self.last.as_ref().and_then(|l| l.drawn.get(i)).map(|d| d.id.clone());
                // A library track: select its card, which has everything.
                match id.and_then(|id| self.cards.iter().position(|c| c.entry.config_path() == id)) {
                    Some(card) => (self.selected, self.drawn_selected) = (card, None),
                    None => self.drawn_selected = Some(i),
                }
                Ok(None)
            }
            Action::Keep => match self.selected_generated().and_then(|d| Some((d.seed()?, d.track.as_ref().ok()?))) {
                Some((seed, track)) => {
                    let def = track.def.clone();
                    library::keep(&def, seed).map(|path| {
                        self.rescan();
                        if let Some(i) = self.cards.iter().position(|c| library::same_file(&c.entry.path, &path)) {
                            (self.selected, self.drawn_selected, self.worst_selected) = (i, None, None);
                            if self.tab == Tab::Library {
                                self.scroll_to_selected();
                            }
                        }
                        Some(format!("kept as {}", path.file_name().unwrap_or_default().to_string_lossy()))
                    })
                }
                None => Ok(None),
            },
            Action::Drive => match card {
                Some(c) if c.entry.track.is_ok() => {
                    self.switch = Some(Switch::Drive(c.entry.path.clone()));
                    Ok(None)
                }
                Some(_) => Err("that track doesn't build".into()),
                None => Ok(None),
            },
            Action::SetRole(role) => match (card, &mut self.set) {
                (Some(c), Ok(set)) => {
                    let name = c.entry.name.clone();
                    set.set_role(&c.entry.config_path(), role).map(|()| {
                        Some(match role {
                            Role::Fixed => format!("{name}: driven every generation"),
                            Role::Pool => format!("{name}: in the pool"),
                            Role::Off => format!("{name}: not trained on"),
                        })
                    })
                }
                (_, Err(err)) => Err(err.clone()),
                (None, _) => Ok(None),
            },
            Action::ToggleTest => match card {
                Some(c) => {
                    let (path, test, name) = (c.entry.path.clone(), c.entry.test, c.entry.name.clone());
                    library::set_test(&path, !test).map(|()| {
                        self.rescan();
                        Some(format!("{name} is {} the test set", if test { "out of" } else { "in" }))
                    })
                }
                None => Ok(None),
            },
            Action::Rename(name) => match card {
                Some(c) => {
                    let path = c.entry.path.clone();
                    library::rename(&path, &name).map(|()| {
                        self.rescan();
                        Some(format!("renamed to {}", name.trim()))
                    })
                }
                None => Ok(None),
            },
            Action::Delete if !self.confirm_delete => {
                self.confirm_delete = true;
                Ok(None)
            }
            Action::Delete => match card {
                Some(c) => {
                    let path = c.entry.path.clone();
                    // A deleted track leaves the training set too.
                    if let Ok(set) = &mut self.set
                        && set.trained_on(&c.entry.config_path())
                    {
                        let _ = set.set_role(&c.entry.config_path(), Role::Off);
                    }
                    self.confirm_delete = false;
                    library::delete(&path).map(|_| {
                        self.deleted.push(path.clone());
                        self.rescan();
                        self.selected = self.selected.min(self.cards.len().saturating_sub(1));
                        Some(format!("moved {} to tracks/deleted/", path.file_name().unwrap_or_default().to_string_lossy()))
                    })
                }
                None => Ok(None),
            },
            Action::CycleSet(step) => {
                let current = self.set.as_ref().map(|s| s.source.clone()).ok();
                let n = self.sources.len() as i32;
                let at = current.and_then(|c| self.sources.iter().position(|s| *s == c)).unwrap_or(0) as i32;
                let next = self.sources[(at + step).rem_euclid(n) as usize].clone();
                self.last = next.run().map(LastGeneration::new);
                self.drawn_selected = None;
                self.worst_selected = None;
                if let Some(run) = next.run() {
                    if self.tab == Tab::Results {
                        self.results = Some(RunResults::new(run));
                    }
                } else {
                    self.tab = Tab::Library;
                }
                self.scroll = 0.0;
                self.set = TrainingSet::load(next);
                self.training_checked = f64::MAX;
                Ok(None)
            }
            Action::Pooled(step) => match &mut self.set {
                Ok(set) => {
                    let pooled = set.pooled.saturating_add_signed(step).min(set.pool.len() as u32);
                    set.set_pooled(pooled).map(|()| None)
                }
                Err(err) => Err(err.clone()),
            },
            Action::Generated(step) => match &mut self.set {
                Ok(set) => {
                    let generated = set.generated.saturating_add_signed(step).min(20);
                    set.set_generated(generated).map(|()| None)
                }
                Err(err) => Err(err.clone()),
            },
        };
        match result {
            Ok(Some(message)) => self.toast = Some((message, 3.0)),
            Ok(None) => {}
            Err(err) => self.toast = Some((err, 6.0)),
        }
    }

    pub fn draw(&mut self) {
        clear_background(theme::GROUND);
        let mut actions = vec![];
        match self.tab {
            Tab::Library => {
                self.draw_grid(&mut actions);
                self.draw_detail(&mut actions);
            }
            Tab::Results => {
                self.draw_results_grid(&mut actions);
                self.draw_results_detail(&mut actions);
            }
        }
        // Over the grid, so cards scrolled up slide under it.
        self.draw_top_bar(&mut actions);
        self.draw_footer();
        for action in actions {
            self.apply(action);
        }
    }

    fn draw_top_bar(&self, actions: &mut Vec<Action>) {
        let w = screen_width();
        draw_rectangle(0.0, 0.0, w, TOP - GAP, theme::GROUND);
        draw_rectangle(0.0, TOP - GAP - 1.0, w, 1.0, theme::GRID_MAJOR);
        text("TRACKS", 16.0, 44.0, 36.0, theme::NEON_CYAN);

        let mut x = 170.0;
        let locked = self.locked || self.sources.len() <= 1;
        if !locked && ui::button(Rect::new(x, 14.0, 34.0, 38.0), "<", ButtonStyle::Normal) {
            actions.push(Action::CycleSet(-1));
        }
        if !locked {
            x += 40.0;
        }
        let label = match &self.set {
            Ok(set) => set.source.label(),
            Err(_) => "(unreadable training set)".into(),
        };
        text("TRAINING SET", x, 22.0, 14.0, theme::TEXT_DIM);
        text(&ui::fit(&label, 300.0, 24.0), x, 46.0, 24.0, theme::TEXT);
        x += 310.0;
        if !locked && ui::button(Rect::new(x, 14.0, 34.0, 38.0), ">", ButtonStyle::Normal) {
            actions.push(Action::CycleSet(1));
        }
        x += 60.0;

        if !self.locked {
            let tab_w = (PANE_W - GAP) / 2.0;
            let tabs = [(Tab::Library, "LIBRARY"), (Tab::Results, "RESULTS")];
            for (i, (tab, label)) in tabs.into_iter().enumerate() {
                let r = Rect::new(w - PANE_W - 16.0 + i as f32 * (tab_w + GAP), 14.0, tab_w, 38.0);
                let style = if self.tab == tab { ButtonStyle::Primary } else { ButtonStyle::Normal };
                if ui::button(r, label, style) && self.tab != tab {
                    actions.push(Action::SwitchTab);
                }
            }
        }

        if self.tab == Tab::Results
            && let Some(results) = &self.results
        {
            let window = results.window().map_or("all".to_owned(), |n| n.to_string());
            if let Some(step) = stepper(x, "LAST N GENS", &window) {
                actions.push(Action::Window(step));
            }
            let line = match (results.first(), results.newest) {
                (Some(first), Some(newest)) => format!(
                    "{}   ·   generations {first}–{newest}   ·   {} runs   ·   {} library tracks",
                    results.run,
                    results.runs(),
                    results.library.len()
                ),
                _ => format!("{}: nothing in tracks.csv yet", results.run),
            };
            text(&line, 16.0, 80.0, 18.0, theme::TEXT_DIM);
        } else if let Ok(set) = &self.set {
            let pool = format!("{} of {}", set.pool_draw(), set.pool.len());
            if let Some(step) = stepper(x, "POOL TRACKS / GEN", &pool) {
                actions.push(Action::Pooled(step));
            }
            x += 200.0;
            if let Some(step) = stepper(x, "GENERATED / GEN", &set.generated.to_string()) {
                actions.push(Action::Generated(step));
            }

            let test_set = self.cards.iter().filter(|c| c.entry.test && !set.trained_on(&c.entry.config_path())).count();
            let line = format!("trains on {}   ·   test set: {test_set} tracks", set.summary());
            let (note, color) = if !set.has_tracks() {
                ("   ·   no training tracks: the trainer will refuse to start".to_owned(), theme::SATURATED)
            } else if self.training {
                (format!("   ·   {} is training: changes apply when it resumes", set.source.run().unwrap_or("")), theme::SODIUM)
            } else if set.source.run().is_some() {
                ("   ·   changes apply when it resumes".to_owned(), theme::TEXT_DIM)
            } else {
                (String::new(), theme::TEXT_DIM)
            };
            let width = text(&line, 16.0, 80.0, 18.0, theme::TEXT_DIM);
            text(&note, 16.0 + width, 80.0, 18.0, color);
        } else if let Err(err) = &self.set {
            text(&ui::fit(err, w - 32.0, 18.0), 16.0, 80.0, 18.0, theme::SATURATED);
        }
    }

    fn grid_rect(&self) -> Rect {
        Rect::new(16.0, TOP, screen_width() - PANE_W - 48.0, screen_height() - TOP - 30.0)
    }

    /// The generated track selected on this tab: in the strip, or among the
    /// worst generated.
    fn selected_generated(&self) -> Option<&Drawn> {
        match self.tab {
            Tab::Library => self.last.as_ref()?.drawn.get(self.drawn_selected?),
            Tab::Results => self.results.as_ref()?.worst.get(self.worst_selected?),
        }
    }

    /// Wheel-scroll the grid over `content` pixels of content. Returns
    /// whether the mouse is over it.
    fn scroll_grid(&mut self, area: Rect, content: f32) -> bool {
        let over = area.contains(mouse_position().into());
        if over && self.renaming.is_none() {
            let (_, wheel) = mouse_wheel();
            self.scroll -= wheel.signum() * SCROLL_STEP * f32::from(wheel != 0.0);
        }
        self.scroll = self.scroll.clamp(0.0, (content - area.h).max(0.0));
        over
    }

    fn draw_grid(&mut self, actions: &mut Vec<Action>) {
        let area = self.grid_rect();
        let (cols, card_w) = columns(area.w, CARD_W);
        self.cols = cols;
        let (small_cols, small_w) = columns(area.w, SMALL_W);
        let drawn = self.last.as_ref().map_or(0, |l| l.drawn.len());
        // The strip (if any), then the library.
        self.library_top =
            if drawn > 0 { SECTION_H + drawn.div_ceil(small_cols) as f32 * (SMALL_H + GAP) + GAP + SECTION_H } else { 0.0 };
        let rows = self.cards.len().div_ceil(cols);
        let content = self.library_top + rows as f32 * (CARD_H + GAP) - GAP;

        let over = self.scroll_grid(area, content);
        let (mx, my) = mouse_position();
        let visible = |y: f32, h: f32| y + h >= area.y - GAP && y <= area.y + area.h;
        let clicked = |r: Rect| over && r.contains(vec2(mx, my)) && is_mouse_button_pressed(MouseButton::Left);

        if let Some(last) = self.last.as_ref().filter(|_| drawn > 0) {
            let y = area.y - self.scroll;
            let title = format!(
                "GENERATION {}  ·  WHAT {}'S SAVED DRIVER DROVE",
                last.generation.unwrap_or(0),
                last.run.to_uppercase()
            );
            text(&title, area.x, y + 20.0, 20.0, theme::SODIUM);
            for (i, d) in last.drawn.iter().enumerate() {
                let x = area.x + (i % small_cols) as f32 * (small_w + GAP);
                let y = y + SECTION_H + (i / small_cols) as f32 * (SMALL_H + GAP);
                if !visible(y, SMALL_H) {
                    continue;
                }
                let r = Rect::new(x, y, small_w, SMALL_H);
                if clicked(r) {
                    actions.push(Action::SelectDrawn(i));
                }
                let selected = match self.drawn_selected {
                    Some(s) => s == i,
                    None => self.cards.get(self.selected).is_some_and(|c| c.entry.config_path() == d.id),
                };
                let result = format!("{:.2} laps  ·  {}", d.laps, d.end);
                draw_drawn_card(d, &result, r, selected, over && r.contains(vec2(mx, my)));
            }
            text("LIBRARY", area.x, area.y - self.scroll + self.library_top - 10.0, 20.0, theme::SODIUM);
        }

        let set = self.set.as_ref().ok();
        for (i, card) in self.cards.iter().enumerate() {
            let x = area.x + (i % cols) as f32 * (card_w + GAP);
            let y = area.y + self.library_top + (i / cols) as f32 * (CARD_H + GAP) - self.scroll;
            if y + CARD_H < area.y - GAP || y > area.y + area.h {
                continue;
            }
            let r = Rect::new(x, y, card_w, CARD_H);
            let hot = over && r.contains(vec2(mx, my));
            if hot && is_mouse_button_pressed(MouseButton::Left) {
                actions.push(Action::Select(i));
            }
            let role = set.map_or(Role::Off, |s| s.role(&card.entry.config_path()));
            draw_card(&card.face(role), r, i == self.selected && self.drawn_selected.is_none(), hot);
        }
    }

    /// Results: library tracks worst first, then the worst generated tracks.
    fn draw_results_grid(&mut self, actions: &mut Vec<Action>) {
        let area = self.grid_rect();
        let (cols, card_w) = columns(area.w, CARD_W);
        let (small_cols, small_w) = columns(area.w, SMALL_W);
        let Some((library, worst)) = self.results.as_ref().map(|r| (r.library.len(), r.worst.len())) else { return };
        let worst_top = SECTION_H + library.max(1).div_ceil(cols) as f32 * (CARD_H + GAP) + GAP;
        let content = worst_top + SECTION_H + worst.div_ceil(small_cols) as f32 * (SMALL_H + GAP);
        let over = self.scroll_grid(area, content);
        let Some(results) = &self.results else { return };
        let top = area.y - self.scroll;
        if results.newest.is_none() {
            let why = format!("no results yet: {} hasn't logged a generation to tracks.csv", results.run);
            text(&why, area.x, top + 20.0, 20.0, theme::TEXT_DIM);
            return;
        }

        let (mx, my) = mouse_position();
        let visible = |y: f32, h: f32| y + h >= area.y - GAP && y <= area.y + area.h;
        let window = window_label(results.window()).to_uppercase();
        text(&format!("LIBRARY TRACKS  ·  WORST FIRST  ·  {window}"), area.x, top + 20.0, 20.0, theme::SODIUM);
        if results.library.is_empty() {
            text("no library tracks driven in this window", area.x, top + SECTION_H + 24.0, 18.0, theme::TEXT_DIM);
        }
        let set = self.set.as_ref().ok();
        let selected_id = self.cards.get(self.selected).filter(|_| self.worst_selected.is_none()).map(|c| c.entry.config_path());
        for (i, t) in results.library.iter().enumerate() {
            let x = area.x + (i % cols) as f32 * (card_w + GAP);
            let y = top + SECTION_H + (i / cols) as f32 * (CARD_H + GAP);
            if !visible(y, CARD_H) {
                continue;
            }
            let r = Rect::new(x, y, card_w, CARD_H);
            let hot = over && r.contains(vec2(mx, my));
            if hot && is_mouse_button_pressed(MouseButton::Left) {
                actions.push(Action::SelectResult(i));
            }
            let s = &t.summary;
            let face = CardFace {
                name: t.name(),
                detail: format!("{:.2} laps  ·  {:.0}% early  ·  {}", s.laps, s.early * 100.0, s.common_end),
                thumb: t.thumb.as_ref(),
                role: set.map_or(Role::Off, |set| set.role(&s.id)),
                test: self.cards.iter().any(|c| c.entry.test && c.entry.config_path() == s.id),
            };
            draw_card(&face, r, selected_id.as_deref() == Some(s.id.as_str()), hot);
        }

        let y = top + worst_top;
        let building = match results.building() {
            0 => String::new(),
            n => format!("  ·  BUILDING {n} MORE"),
        };
        text(&format!("WORST GENERATED  ·  FEWEST LAPS  ·  {window}{building}"), area.x, y + 20.0, 20.0, theme::SODIUM);
        for (i, d) in results.worst.iter().enumerate() {
            let x = area.x + (i % small_cols) as f32 * (small_w + GAP);
            let y = y + SECTION_H + (i / small_cols) as f32 * (SMALL_H + GAP);
            if !visible(y, SMALL_H) {
                continue;
            }
            let r = Rect::new(x, y, small_w, SMALL_H);
            let hot = over && r.contains(vec2(mx, my));
            if hot && is_mouse_button_pressed(MouseButton::Left) {
                actions.push(Action::SelectWorst(i));
            }
            let result = format!("g{}  ·  {:.2} laps  ·  {}", d.generation, d.laps, d.end);
            draw_drawn_card(d, &result, r, self.worst_selected == Some(i), hot);
        }
    }

    fn draw_detail(&mut self, actions: &mut Vec<Action>) {
        let x = screen_width() - PANE_W - 16.0;
        let map = Rect::new(x, TOP, PANE_W, pane_map_h());
        let ui = &mut self.panel;
        (ui.x, ui.y) = (x, TOP + map.h + GAP);
        if let Some(d) = self.drawn_selected.and_then(|i| self.last.as_ref()?.drawn.get(i)) {
            generated_pane(ui, map, d, "This generation", actions);
            return;
        }
        let Some(card) = self.cards.get(self.selected) else {
            draw_pane_map(map, None);
            text("no tracks in tracks/", x + 16.0, TOP + 40.0, 20.0, theme::TEXT_DIM);
            return;
        };
        draw_pane_map(map, card.thumb.as_ref());

        let e = &card.entry;
        let config_path = e.config_path();
        ui.begin(&e.name.to_uppercase(), screen_height() - 30.0);
        stats_labels(ui, card.stats.as_ref(), e.track.as_ref().err());
        if let Some(last) = &self.last {
            ui.heading(&format!("Generation {}", last.generation.unwrap_or(0)));
            match last.row(&config_path) {
                Some(d) => ui.label(&run_line(d), theme::TEXT),
                None => ui.label(&format!("not driven by {}'s saved driver", last.run), theme::TEXT_DIM),
            }
        }

        if let Ok(set) = &self.set {
            training_section(ui, set, &config_path, e.test, actions);
        }

        ui.heading("Track");
        if let Some(name) = &mut self.renaming {
            ui.label("new name (Enter saves, Esc cancels)", theme::TEXT_DIM);
            match ui.text_field(name, true) {
                Some(true) => {
                    actions.push(Action::Rename(name.clone()));
                    self.renaming = None;
                }
                Some(false) => self.renaming = None,
                None => {}
            }
        } else {
            let test = if e.test { "test set: on (T)" } else { "test set: off (T)" };
            match ui.buttons(&[(test, ButtonStyle::Normal), ("drive (Enter)", ButtonStyle::Primary)]) {
                Some(0) => actions.push(Action::ToggleTest),
                Some(1) => actions.push(Action::Drive),
                _ => {}
            }
            let delete = if self.confirm_delete { "really delete?" } else { "delete" };
            let delete_style = if self.confirm_delete { ButtonStyle::Primary } else { ButtonStyle::Normal };
            match ui.buttons(&[("rename", ButtonStyle::Normal), (delete, delete_style)]) {
                Some(0) => self.renaming = Some(e.name.clone()),
                Some(1) => actions.push(Action::Delete),
                _ => {}
            }
        }
        ui.end();
    }

    /// Results pane: a worst generated track, or the selected library
    /// track's results.
    fn draw_results_detail(&mut self, actions: &mut Vec<Action>) {
        let x = screen_width() - PANE_W - 16.0;
        let map = Rect::new(x, TOP, PANE_W, pane_map_h());
        let ui = &mut self.panel;
        (ui.x, ui.y) = (x, TOP + map.h + GAP);
        let results = self.results.as_ref();
        if let Some(d) = self.worst_selected.and_then(|i| results?.worst.get(i)) {
            generated_pane(ui, map, d, &format!("Generation {}", d.generation), actions);
            return;
        }
        let Some(card) = self.cards.get(self.selected) else {
            draw_pane_map(map, None);
            text("no tracks in tracks/", x + 16.0, TOP + 40.0, 20.0, theme::TEXT_DIM);
            return;
        };
        draw_pane_map(map, card.thumb.as_ref());

        let e = &card.entry;
        let config_path = e.config_path();
        ui.begin(&e.name.to_uppercase(), screen_height() - 30.0);
        stats_labels(ui, card.stats.as_ref(), e.track.as_ref().err());
        ui.heading(&window_label(results.and_then(|r| r.window())));
        match results.and_then(|r| r.library.iter().find(|t| t.summary.id == config_path)) {
            Some(TrackResult { summary: s, .. }) => {
                ui.label(&format!("{} runs: {:.2} laps on average ({:.0} m)", s.runs, s.laps, s.progress), theme::TEXT);
                ui.label(&format!("{:.0}% ended early   ·   most often: {}", s.early * 100.0, s.common_end), theme::TEXT);
                for (role, runs, laps) in &s.by_role {
                    ui.label(&format!("{role}: {runs} runs, {laps:.2} laps"), theme::TEXT_DIM);
                }
                if let Some(area) = ui.area(SPARK_H) {
                    let generations: Vec<f64> = (0..s.laps_by_gen.len()).map(|g| g as f64).collect();
                    let chart = Chart {
                        title: "Laps per generation",
                        unit: "",
                        series: vec![Series { name: "laps", values: &s.laps_by_gen, dots: true }],
                        kind: Kind::Lines,
                        range: None,
                    };
                    chart.draw(area, &[], None, &generations);
                }
            }
            None => ui.label("not driven in this window", theme::TEXT_DIM),
        }
        if let Ok(set) = &self.set {
            training_section(ui, set, &config_path, e.test, actions);
        }
        ui.heading("Track");
        if ui.buttons(&[("drive (Enter)", ButtonStyle::Primary)]).is_some() {
            actions.push(Action::Drive);
        }
        ui.end();
    }

    fn draw_footer(&self) {
        let h = screen_height();
        // Over the grid, so cards scrolled down slide under it.
        draw_rectangle(0.0, h - 28.0, screen_width(), 28.0, theme::GROUND);
        let help = match self.tab {
            Tab::Library => {
                "click / arrows select   Enter drive   F fixed   P pool   T test set   K keep   Tab results   [ ] training set   wheel scroll   Esc back"
            }
            Tab::Results => "click select   Enter drive   F fixed   P pool   K keep   Tab library   [ ] training set   wheel scroll   Esc back",
        };
        let help = if self.locked { help.replace("   [ ] training set", "").replace("   Tab results", "").replace("Esc back", "Esc done") } else { help.to_owned() };
        text(&help, 16.0, h - 10.0, 16.0, theme::TEXT_DIM);
        if let Some((message, _)) = &self.toast {
            let width = measure_text(message, None, 22, 1.0).width;
            let x = ((screen_width() - PANE_W - 32.0 - width) / 2.0).max(16.0);
            draw_rectangle(x - 10.0, h - 64.0, width + 20.0, 32.0, theme::PANEL_SOLID);
            text(message, x, h - 41.0, 22.0, theme::NEON_CYAN);
        }
    }
}

/// The detail pane's big map, framed.
fn draw_pane_map(map: Rect, thumb: Option<&Thumb>) {
    draw_rectangle(map.x, map.y, map.w, map.h, theme::PANEL_SOLID);
    draw_rectangle_lines(map.x, map.y, map.w, map.h, 1.0, theme::GRID_MAJOR);
    match thumb {
        Some(thumb) => thumb.draw(Rect::new(map.x + 12.0, map.y + 12.0, map.w - 24.0, map.h - 24.0), ThumbStyle::DETAIL),
        None => {
            text("doesn't build", map.x + 16.0, map.y + 40.0, 20.0, theme::SATURATED);
        }
    }
}

fn stats_labels(ui: &mut Panel, stats: Option<&TrackStats>, error: Option<&String>) {
    if let Some(s) = stats {
        ui.label(&format!("{:.2} km   ·   {} corners ({:.1} per km)", s.length / 1000.0, s.corners, s.corners_per_km), theme::TEXT);
        let sharpest = s.sharpest.map_or("-".into(), |r| format!("r {r:.0} m"));
        ui.label(&format!("sharpest {sharpest}   ·   {} hairpins   ·   {} crossings", s.hairpins, s.crossings), theme::TEXT);
        ui.label(&format!("hard walls on {:.0}% of the edges", s.hard_share * 100.0), theme::TEXT);
    } else if let Some(why) = error {
        ui.label(why, theme::SATURATED);
    }
}

/// The detail pane for a generated track: what it did, and keep.
fn generated_pane(ui: &mut Panel, map: Rect, d: &Drawn, heading: &str, actions: &mut Vec<Action>) {
    draw_pane_map(map, d.thumb.as_ref());
    ui.begin(&d.name().to_uppercase(), screen_height() - 30.0);
    stats_labels(ui, d.stats.as_ref(), d.track.as_ref().err());
    ui.heading(heading);
    ui.label(&run_line(d), theme::TEXT);
    ui.label("a fresh generated track: keep it to drive it", theme::TEXT_DIM);
    if ui.buttons(&[("keep in library (K)", ButtonStyle::Primary)]).is_some() {
        actions.push(Action::Keep);
    }
    ui.end();
}

/// The selected library track's role in the training set.
fn training_section(ui: &mut Panel, set: &TrainingSet, config_path: &str, test: bool, actions: &mut Vec<Action>) {
    ui.heading("Training");
    let role = set.role(config_path);
    let style = |r: Role| if role == r { ButtonStyle::Primary } else { ButtonStyle::Normal };
    match ui.buttons(&[("fixed (F)", style(Role::Fixed)), ("pool (P)", style(Role::Pool)), ("off", style(Role::Off))]) {
        Some(0) => actions.push(Action::SetRole(Role::Fixed)),
        Some(1) => actions.push(Action::SetRole(Role::Pool)),
        Some(2) => actions.push(Action::SetRole(Role::Off)),
        _ => {}
    }
    let about = match role {
        Role::Fixed => "driven every generation".to_owned(),
        Role::Pool => format!("one of {}; {} drawn each generation", set.pool.len(), set.pool_draw()),
        Role::Off if test => "not trained on: in the test set".to_owned(),
        Role::Off => "not trained on".to_owned(),
    };
    ui.label(&about, theme::TEXT_DIM);
}

/// How many `item_w` columns fit in `width`, and their stretched width.
fn columns(width: f32, item_w: f32) -> (usize, f32) {
    let cols = (((width + GAP) / (item_w + GAP)).floor() as usize).max(1);
    (cols, (width - GAP * (cols as f32 - 1.0)) / cols as f32)
}

/// "Last 20 gens", or "Whole run".
fn window_label(window: Option<u32>) -> String {
    window.map_or("Whole run".to_owned(), |n| format!("Last {n} gens"))
}

/// "pool: 0.12 laps (180 m), crashed".
fn run_line(d: &Drawn) -> String {
    format!("{}: {:.2} laps ({:.0} m), {}", d.role, d.laps, d.progress, d.end)
}

fn role_color(role: &str) -> Color {
    match role {
        "fixed" => theme::NEON_PINK,
        "pool" => theme::NEON_CYAN,
        "generated" => theme::GRIP,
        _ => theme::TEXT_DIM,
    }
}

/// A small card for a generated (or this generation's) track; `result` is
/// its line under the name.
fn draw_drawn_card(d: &Drawn, result: &str, r: Rect, selected: bool, hot: bool) {
    draw_rectangle(r.x, r.y, r.w, r.h, if hot { theme::GRID_MINOR } else { theme::PANEL_SOLID });
    if selected {
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, theme::NEON_PINK);
    } else {
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, theme::GRID_MAJOR);
    }
    let map = Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, SMALL_MAP_H);
    match &d.thumb {
        Some(thumb) => thumb.draw(map, ThumbStyle::CARD),
        None => {
            text("doesn't build", map.x + 4.0, map.y + 20.0, 16.0, theme::SATURATED);
        }
    }
    ui::chip(r.x + 6.0, r.y + 6.0, &d.role.to_uppercase(), role_color(&d.role));
    text(&ui::fit(&d.name(), r.w - 16.0, 16.0), r.x + 8.0, r.y + SMALL_MAP_H + 28.0, 16.0, theme::TEXT);
    text(&ui::fit(result, r.w - 16.0, 14.0), r.x + 8.0, r.y + SMALL_MAP_H + 44.0, 14.0, theme::TEXT_DIM);
}

/// The detail pane's map: big, but leaving the panel below room.
fn pane_map_h() -> f32 {
    ((screen_height() - TOP) * 0.36).clamp(180.0, 340.0)
}

/// `label` over "[-] value [+]". Returns -1 / +1 when clicked.
fn stepper(x: f32, label: &str, value: &str) -> Option<i32> {
    text(label, x, 22.0, 14.0, theme::TEXT_DIM);
    let minus = ui::button(Rect::new(x, 28.0, 30.0, 26.0), "-", ButtonStyle::Normal);
    let w = measure_text(value, None, 22, 1.0).width;
    text(value, x + 36.0 + (70.0 - w) / 2.0, 48.0, 22.0, theme::TEXT);
    let plus = ui::button(Rect::new(x + 112.0, 28.0, 30.0, 26.0), "+", ButtonStyle::Normal);
    if minus {
        Some(-1)
    } else if plus {
        Some(1)
    } else {
        None
    }
}

fn draw_card(face: &CardFace, r: Rect, selected: bool, hot: bool) {
    let fill = if hot { theme::GRID_MINOR } else { theme::PANEL_SOLID };
    draw_rectangle(r.x, r.y, r.w, r.h, fill);
    if selected {
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, theme::NEON_PINK);
    } else {
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, theme::GRID_MAJOR);
    }

    let map = Rect::new(r.x + 10.0, r.y + 10.0, r.w - 20.0, CARD_MAP_H);
    let role = face.role;
    match face.thumb {
        Some(thumb) => thumb.draw(map, if role == Role::Off { ThumbStyle::DIM } else { ThumbStyle::CARD }),
        None => {
            text("doesn't build", map.x + 8.0, map.y + 24.0, 18.0, theme::SATURATED);
        }
    }

    let mut x = r.x + 8.0;
    let y = r.y + 8.0;
    match role {
        Role::Fixed => x += ui::chip(x, y, "FIXED", theme::NEON_PINK) + 4.0,
        Role::Pool => x += ui::chip(x, y, "POOL", theme::NEON_CYAN) + 4.0,
        Role::Off => {}
    }
    if face.test && role == Role::Off {
        ui::chip(x, y, "TEST", theme::SODIUM);
    }

    let name_color = if role == Role::Off { theme::TEXT_DIM } else { theme::TEXT };
    text(&ui::fit(&face.name, r.w - 20.0, 20.0), r.x + 10.0, r.y + CARD_MAP_H + 36.0, 20.0, name_color);
    text(&ui::fit(&face.detail, r.w - 20.0, 15.0), r.x + 10.0, r.y + CARD_MAP_H + 56.0, 15.0, theme::TEXT_DIM);
}
