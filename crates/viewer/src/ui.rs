//! A small immediate-mode UI for tool panels: themed, thick sliders, buttons
//! and collapsible sections. Each frame, call widgets in order between
//! `begin` and `end`; each one handles its input and draws itself.

use macroquad::prelude::*;

use crate::render::{text, theme};

const PAD: f32 = 14.0;
const TITLE_HEIGHT: f32 = 40.0;
const HEADING_HEIGHT: f32 = 32.0;
const SLIDER_HEIGHT: f32 = 42.0;
const BAR_HEIGHT: f32 = 12.0;
const BUTTON_HEIGHT: f32 = 34.0;
const ITEM_HEIGHT: f32 = 34.0;
const FIELD_HEIGHT: f32 = 36.0;
const GAP: f32 = 8.0;
const TEXT_SIZE: f32 = 18.0;
const SCROLL_STEP: f32 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// Filled neon: the main action.
    Primary,
    Normal,
}

pub struct Panel {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    scroll: f32,
    /// Height of the content as measured last frame.
    content_height: f32,
    /// Where the next row goes, relative to the top of the content.
    cursor: f32,
    /// Visible area of this frame's content: top and bottom on screen.
    view: (f32, f32),
    /// Slider being dragged.
    active: Option<u64>,
    collapsed: Vec<u64>,
    /// Whether rows under the current heading are shown.
    section_open: bool,
    hovered: bool,
}

impl Panel {
    pub fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            x,
            y,
            width,
            scroll: 0.0,
            content_height: 0.0,
            cursor: 0.0,
            view: (0.0, 0.0),
            active: None,
            collapsed: vec![],
            section_open: true,
            hovered: false,
        }
    }

    /// Whether the mouse was over the panel (so the app shouldn't also treat
    /// clicks and scrolling as its own).
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn collapse(&mut self, heading: &str) {
        let id = id_of(heading);
        if !self.collapsed.contains(&id) {
            self.collapsed.push(id);
        }
    }

    /// Start a frame: draws the frame and title, handles scrolling.
    pub fn begin(&mut self, title: &str, max_bottom: f32) {
        let content_top = self.y + TITLE_HEIGHT;
        let height = (self.content_height + TITLE_HEIGHT + PAD).min(max_bottom - self.y).max(TITLE_HEIGHT + 60.0);
        let bottom = self.y + height;
        self.view = (content_top, bottom - PAD / 2.0);

        let (mx, my) = mouse_position();
        self.hovered = mx >= self.x && mx <= self.x + self.width && my >= self.y && my <= bottom;
        if self.hovered {
            let (_, wheel) = mouse_wheel();
            self.scroll -= wheel.signum() * SCROLL_STEP * f32::from(wheel != 0.0);
        }
        let max_scroll = (self.content_height - (self.view.1 - self.view.0)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        if !is_mouse_button_down(MouseButton::Left) {
            self.active = None;
        }

        draw_rectangle(self.x, self.y, self.width, height, theme::PANEL_SOLID);
        draw_rectangle(self.x, self.y, self.width, 3.0, theme::NEON_PINK);
        draw_rectangle_lines(self.x, self.y, self.width, height, 1.0, theme::GRID_MAJOR);
        text(title, self.x + PAD, self.y + 28.0, 24.0, theme::NEON_CYAN);
        if max_scroll > 0.0 {
            // Scroll position marker.
            let track = self.view.1 - self.view.0;
            let thumb = track * track / self.content_height;
            let at = self.view.0 + (track - thumb) * self.scroll / max_scroll;
            draw_rectangle(self.x + self.width - 4.0, at, 3.0, thumb, theme::TEXT_DIM);
        }
        self.cursor = 0.0;
        self.section_open = true;
    }

    pub fn end(&mut self) {
        self.content_height = self.cursor;
    }

    /// Reserve a row; returns its top on screen, or `None` if scrolled out
    /// of view (it still takes up space).
    fn row(&mut self, height: f32) -> Option<f32> {
        let top = self.view.0 + self.cursor - self.scroll;
        self.cursor += height;
        (top >= self.view.0 - 0.5 && top + height <= self.view.1 + 0.5).then_some(top)
    }

    fn clicked(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        is_mouse_button_pressed(MouseButton::Left) && inside(x, y, w, h)
    }

    /// A free line of text.
    pub fn label(&mut self, line: &str, color: Color) {
        if !self.section_open {
            return;
        }
        if let Some(top) = self.row(26.0) {
            text(&fit(line, self.width - 2.0 * PAD, TEXT_SIZE), self.x + PAD, top + 18.0, TEXT_SIZE, color);
        }
    }

    /// A collapsible section heading. Rows until the next heading are only
    /// shown while it's open; click it to toggle.
    pub fn heading(&mut self, title: &str) -> bool {
        let id = id_of(title);
        let mut open = !self.collapsed.contains(&id);
        if let Some(top) = self.row(HEADING_HEIGHT) {
            let (x, w) = (self.x + PAD, self.width - 2.0 * PAD);
            if self.clicked(x, top, w, HEADING_HEIGHT - 4.0) {
                open = !open;
                if open {
                    self.collapsed.retain(|&c| c != id);
                } else {
                    self.collapsed.push(id);
                }
            }
            draw_rectangle(x, top + HEADING_HEIGHT - 6.0, w, 1.0, theme::GRID_MAJOR);
            let marker = if open { "-" } else { "+" };
            let color = if inside(x, top, w, HEADING_HEIGHT) { theme::TEXT } else { theme::SODIUM };
            text(&format!("{marker} {}", title.to_uppercase()), x, top + 21.0, 20.0, color);
        }
        self.section_open = open;
        open
    }

    /// A thick slider with its label and value. Returns true if it changed.
    pub fn slider(&mut self, label: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, decimals: usize) -> bool {
        if !self.section_open {
            return false;
        }
        let id = id_of(label);
        let Some(top) = self.row(SLIDER_HEIGHT) else { return false };
        let (x, w) = (self.x + PAD, self.width - 2.0 * PAD - 6.0);
        let (lo, hi) = (*range.start(), *range.end());

        let bar_y = top + 22.0;
        let grab_h = BAR_HEIGHT + 10.0;
        if self.clicked(x - 4.0, bar_y - 5.0, w + 8.0, grab_h) {
            self.active = Some(id);
        }
        let mut changed = false;
        if self.active == Some(id) {
            let t = ((mouse_position().0 - x) / w).clamp(0.0, 1.0);
            let raw = lo + (hi - lo) * f64::from(t);
            let step = 10f64.powi(-(decimals as i32));
            let snapped = (raw / step).round() * step;
            if (snapped - *value).abs() > step / 2.0 {
                *value = snapped;
                changed = true;
            }
        }

        let shown = format!("{:.*}", decimals, *value);
        let value_width = measure_text(&shown, None, TEXT_SIZE as u16, 1.0).width;
        text(&fit(label, w - value_width - 12.0, TEXT_SIZE), x, top + 15.0, TEXT_SIZE, theme::TEXT);
        text(&shown, x + w - value_width, top + 15.0, TEXT_SIZE, theme::NEON_CYAN);

        let hot = self.active == Some(id) || inside(x, bar_y - 5.0, w, grab_h);
        let t = (((*value - lo) / (hi - lo)) as f32).clamp(0.0, 1.0);
        draw_rectangle(x, bar_y, w, BAR_HEIGHT, theme::GRID_MAJOR);
        let fill = if hot { theme::NEON_PINK } else { Color { a: 0.75, ..theme::NEON_PINK } };
        draw_rectangle(x, bar_y, w * t, BAR_HEIGHT, fill);
        let knob_x = x + w * t - 3.0;
        draw_rectangle(knob_x, bar_y - 4.0, 6.0, BAR_HEIGHT + 8.0, if hot { WHITE } else { theme::TEXT });
        changed
    }

    /// Whole-number slider.
    pub fn int_slider(&mut self, label: &str, value: &mut u32, range: std::ops::RangeInclusive<u32>) -> bool {
        let mut v = f64::from(*value);
        let changed = self.slider(label, &mut v, f64::from(*range.start())..=f64::from(*range.end()), 0);
        if changed {
            *value = v.round() as u32;
        }
        changed
    }

    /// A selectable list row: label on the left, detail on the right.
    /// Returns true when clicked.
    pub fn list_item(&mut self, label: &str, detail: &str, selected: bool) -> bool {
        self.list_item_with(label, detail, selected, None)
    }

    /// A list item with a colour swatch before its label.
    pub fn list_item_swatch(&mut self, label: &str, detail: &str, selected: bool, swatch: Color) -> bool {
        self.list_item_with(label, detail, selected, Some(swatch))
    }

    fn list_item_with(&mut self, label: &str, detail: &str, selected: bool, swatch: Option<Color>) -> bool {
        if !self.section_open {
            return false;
        }
        let Some(top) = self.row(ITEM_HEIGHT) else { return false };
        let (x, w, h) = (self.x + PAD, self.width - 2.0 * PAD, ITEM_HEIGHT - 4.0);
        let hot = inside(x, top, w, h);
        if selected {
            draw_rectangle(x, top, w, h, Color { a: 0.22, ..theme::NEON_PINK });
            draw_rectangle(x, top, 3.0, h, theme::NEON_PINK);
        } else if hot {
            draw_rectangle(x, top, w, h, theme::GRID_MAJOR);
        }
        let detail_w = measure_text(detail, None, 14, 1.0).width;
        let mut label_x = x + 10.0;
        if let Some(color) = swatch {
            draw_rectangle(label_x, top + h / 2.0 - 5.0, 10.0, 10.0, color);
            label_x += 18.0;
        }
        text(&fit(label, w - detail_w - 24.0 - (label_x - x - 10.0), TEXT_SIZE), label_x, top + 21.0, TEXT_SIZE, theme::TEXT);
        text(detail, x + w - detail_w - 8.0, top + 20.0, 14.0, theme::TEXT_DIM);
        self.clicked(x, top, w, h)
    }

    /// A one-line text box for `value`, edited while `editing` is set.
    /// Returns Some(true) on Enter (keep the text), Some(false) on Escape.
    pub fn text_field(&mut self, value: &mut String, editing: bool) -> Option<bool> {
        if !self.section_open {
            return None;
        }
        let top = self.row(FIELD_HEIGHT)?;
        let (x, w, h) = (self.x + PAD, self.width - 2.0 * PAD, FIELD_HEIGHT - 6.0);
        draw_rectangle(x, top, w, h, theme::GRID_MAJOR);
        draw_rectangle_lines(x, top, w, h, 1.0, if editing { theme::NEON_CYAN } else { theme::TEXT_DIM });
        let mut result = None;
        if editing {
            while let Some(c) = get_char_pressed() {
                if !c.is_control() && value.chars().count() < 40 {
                    value.push(c);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                value.pop();
            }
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                result = Some(true);
            }
            if is_key_pressed(KeyCode::Escape) {
                result = Some(false);
            }
        }
        let shown = fit(value, w - 24.0, TEXT_SIZE);
        let caret = if editing && (get_time() * 2.0) as i64 % 2 == 0 { "|" } else { "" };
        text(&format!("{shown}{caret}"), x + 8.0, top + 21.0, TEXT_SIZE, theme::TEXT);
        result
    }

    /// A row of buttons sharing the width. Returns the index clicked.
    pub fn buttons(&mut self, buttons: &[(&str, ButtonStyle)]) -> Option<usize> {
        if !self.section_open || buttons.is_empty() {
            return None;
        }
        let top = self.row(BUTTON_HEIGHT + GAP)?;
        let count = buttons.len() as f32;
        let w = (self.width - 2.0 * PAD - GAP * (count - 1.0)) / count;
        let mut clicked = None;
        for (i, (label, style)) in buttons.iter().enumerate() {
            let x = self.x + PAD + i as f32 * (w + GAP);
            let hot = inside(x, top, w, BUTTON_HEIGHT);
            let (fill, ink) = match (style, hot) {
                (ButtonStyle::Primary, false) => (theme::NEON_PINK, WHITE),
                (ButtonStyle::Primary, true) => (WHITE, theme::NEON_PINK),
                (ButtonStyle::Normal, false) => (theme::GRID_MAJOR, theme::TEXT),
                (ButtonStyle::Normal, true) => (theme::NEON_CYAN, theme::ASPHALT),
            };
            draw_rectangle(x, top, w, BUTTON_HEIGHT, fill);
            let shown = fit(label, w - 8.0, TEXT_SIZE);
            let tw = measure_text(&shown, None, TEXT_SIZE as u16, 1.0).width;
            text(&shown, x + (w - tw) / 2.0, top + 23.0, TEXT_SIZE, ink);
            if self.clicked(x, top, w, BUTTON_HEIGHT) {
                clicked = Some(i);
            }
        }
        clicked
    }
}

fn inside(x: f32, y: f32, w: f32, h: f32) -> bool {
    let (mx, my) = mouse_position();
    mx >= x && mx <= x + w && my >= y && my <= y + h
}

/// Shorten `s` with a trailing ".." until it fits in `width` pixels.
fn fit(s: &str, width: f32, size: f32) -> String {
    let fits = |t: &str| measure_text(t, None, size as u16, 1.0).width <= width;
    if fits(s) {
        return s.to_owned();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "..";
        if fits(&candidate) {
            return candidate;
        }
    }
    String::new()
}

/// Stable id for a widget from its label (FNV-1a).
fn id_of(label: &str) -> u64 {
    label.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}
