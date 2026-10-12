use macroquad::prelude::*;

use crate::input::gamepad::Gamepad;
use crate::render::{text, theme};

/// Raw gamepad readout, for figuring out what an unknown controller sends.
pub fn draw(gamepad: &Gamepad) {
    let lines = gamepad.debug_lines();
    let (x, y) = (16.0, 380.0);
    let h = 34.0 + lines.len() as f32 * 18.0;
    draw_rectangle(x, y, 420.0, h, theme::PANEL);
    text("GAMEPAD", x + 12.0, y + 24.0, 22.0, theme::NEON_CYAN);
    for (i, line) in lines.iter().enumerate() {
        text(line, x + 12.0, y + 46.0 + i as f32 * 18.0, 16.0, theme::TEXT);
    }
}
