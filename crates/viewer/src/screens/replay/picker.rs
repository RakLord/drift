//! Replay picker (O): every replay in the same folder, newest first. Click
//! one to watch it.

use std::path::{Path, PathBuf};

use agent::replay_file;
use macroquad::prelude::*;

use crate::render::theme;
use crate::ui::Panel;

struct Entry {
    path: PathBuf,
    label: String,
    detail: String,
}

pub struct Picker {
    pub open: bool,
    panel: Panel,
    entries: Vec<Entry>,
}

impl Picker {
    pub fn new(open: bool) -> Self {
        Self { open, panel: Panel::new(16.0, 110.0, 400.0), entries: vec![] }
    }

    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Re-read the folder `current` is in.
    pub fn refresh(&mut self, current: &Path) {
        self.entries = list(current.parent().unwrap_or(Path::new(".")))
            .into_iter()
            .map(|path| match replay_file::load_header(&path) {
                Ok(h) => Entry {
                    label: if h.run.is_empty() { h.title.clone() } else { format!("gen {}  {}", h.generation, h.track.name) },
                    detail: format!("{:.0} pts  {:.2} laps  {}", h.result.score.total(), h.result.laps(), h.result.end.name()),
                    path,
                },
                Err(err) => Entry { label: file_name(&path), detail: err.to_string(), path },
            })
            .collect();
    }

    /// The replay before (-1) or after (+1) `current` in the list.
    pub fn neighbour(&self, current: &Path, step: isize) -> Option<PathBuf> {
        let i = self.entries.iter().position(|e| e.path == current)?;
        let j = i.checked_add_signed(step)?;
        self.entries.get(j).map(|e| e.path.clone())
    }

    /// Draw (if open); returns a replay to open if one was clicked.
    pub fn show(&mut self, current: &Path) -> Option<PathBuf> {
        if !self.open {
            return None;
        }
        let ui = &mut self.panel;
        ui.begin("REPLAYS", screen_height() - 60.0);
        ui.label("click to watch   Up/Down: previous/next", theme::TEXT_DIM);
        let mut chosen = None;
        for e in &self.entries {
            let selected = e.path == current;
            if ui.list_item(&e.label, &e.detail, selected) && !selected {
                chosen = Some(e.path.clone());
            }
        }
        if self.entries.is_empty() {
            ui.label("(no replays here)", theme::TEXT_DIM);
        }
        ui.end();
        chosen
    }
}

/// Replay files in `dir`, newest generation first.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut paths: Vec<PathBuf> =
        entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "replay")).collect();
    // g00040_t2.replay: generations newest first, tracks in order.
    let key = |p: &PathBuf| {
        let name = file_name(p);
        let (generation, rest) = name.split_once("_t").unwrap_or((name.as_str(), ""));
        (std::cmp::Reverse(generation.to_owned()), rest.to_owned())
    };
    paths.sort_by_key(key);
    paths
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned())
}
