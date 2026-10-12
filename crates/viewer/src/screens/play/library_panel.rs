//! Library panel (O): every track in `tracks/`. Click one to drive it; for
//! the selected track, toggle whether it's in the trainer's test set, rename
//! it, or delete it (moved to `tracks/deleted/`, with a confirm click).

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use super::library::{self, Entry};
use crate::render::theme;
use crate::ui::{ButtonStyle, Panel};

pub enum LibraryAction {
    Drive(PathBuf),
    /// A file changed; the message says how.
    Changed(String),
    Deleted(PathBuf),
    Error(String),
}

pub struct LibraryPanel {
    pub open: bool,
    panel: Panel,
    entries: Vec<Entry>,
    /// Name being typed, while renaming.
    renaming: Option<String>,
    confirm_delete: bool,
}

impl LibraryPanel {
    pub fn new() -> Self {
        // Beside the generator panel, so both can be open.
        Self { open: false, panel: Panel::new(428.0, 110.0, 400.0), entries: vec![], renaming: None, confirm_delete: false }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.refresh();
        }
        self.renaming = None;
        self.confirm_delete = false;
    }

    pub fn refresh(&mut self) {
        self.entries = library::scan();
    }

    /// Whether the mouse is over the panel.
    pub fn hovered(&self) -> bool {
        self.open && self.panel.hovered()
    }

    /// Whether the panel wants the keyboard (typing a name).
    pub fn typing(&self) -> bool {
        self.open && self.renaming.is_some()
    }

    /// Draw (if open) and handle input. `current` is the library track being
    /// driven, if any.
    pub fn show(&mut self, current: Option<&Path>) -> Option<LibraryAction> {
        if !self.open {
            return None;
        }
        let mut action = None;
        let ui = &mut self.panel;
        ui.begin("TRACK LIBRARY", screen_height() - 16.0);
        ui.label("click a track to drive it", theme::TEXT_DIM);

        let mut selected = None;
        for (i, e) in self.entries.iter().enumerate() {
            let detail = match &e.length {
                Ok(m) => format!("{:.2} km{}", m / 1000.0, if e.test { "  test" } else { "" }),
                Err(_) => "doesn't build".to_owned(),
            };
            let is_current = current.is_some_and(|c| library::same_file(c, &e.path));
            if is_current {
                selected = Some(i);
            }
            if ui.list_item(&e.name, &detail, is_current) && !is_current {
                action = Some(LibraryAction::Drive(e.path.clone()));
                self.renaming = None;
                self.confirm_delete = false;
            }
        }

        if let Some(i) = selected {
            let e = self.entries[i].clone();
            ui.heading(&format!("Selected: {}", e.file_name()));
            if let Err(why) = &e.length {
                ui.label(why, theme::SATURATED);
            }
            if let Some(name) = &mut self.renaming {
                ui.label("new name (Enter saves, Esc cancels)", theme::TEXT_DIM);
                match ui.text_field(name, true) {
                    Some(true) => {
                        action = Some(match library::rename(&e.path, name) {
                            Ok(()) => LibraryAction::Changed(format!("renamed to {}", name.trim())),
                            Err(err) => LibraryAction::Error(err),
                        });
                        self.renaming = None;
                    }
                    Some(false) => self.renaming = None,
                    None => {}
                }
            } else {
                let test_label = if e.test { "test set: on" } else { "test set: off" };
                let delete_label = if self.confirm_delete { "really delete?" } else { "delete" };
                let delete_style = if self.confirm_delete { ButtonStyle::Primary } else { ButtonStyle::Normal };
                match ui.buttons(&[(test_label, ButtonStyle::Normal), ("rename", ButtonStyle::Normal), (delete_label, delete_style)]) {
                    Some(0) => {
                        action = Some(match library::set_test(&e.path, !e.test) {
                            Ok(()) => LibraryAction::Changed(format!(
                                "{} is {} the test set",
                                e.name,
                                if e.test { "out of" } else { "in" }
                            )),
                            Err(err) => LibraryAction::Error(err),
                        });
                    }
                    Some(1) => self.renaming = Some(e.name.clone()),
                    Some(2) if self.confirm_delete => {
                        action = Some(match library::delete(&e.path) {
                            Ok(_) => LibraryAction::Deleted(e.path.clone()),
                            Err(err) => LibraryAction::Error(err),
                        });
                        self.confirm_delete = false;
                    }
                    Some(2) => self.confirm_delete = true,
                    _ => {}
                }
            }
        } else {
            ui.label("(driving a generated track: N / generator)", theme::TEXT_DIM);
        }
        ui.end();

        if matches!(action, Some(LibraryAction::Changed(_) | LibraryAction::Deleted(_))) {
            self.refresh();
        }
        action
    }
}
