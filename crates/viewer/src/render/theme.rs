//! Colours. JDM night: dark asphalt, sodium orange, neon accents.

use macroquad::prelude::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    rgba(r, g, b, 255)
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
}

pub const ASPHALT: Color = rgb(38, 40, 48);
pub const GRID_MINOR: Color = rgb(45, 48, 57);
pub const GRID_MAJOR: Color = rgb(56, 60, 72);
pub const PAINT: Color = rgba(216, 210, 184, 150);
pub const CONE: Color = rgb(255, 122, 26);

pub const CAR_BODY: Color = rgb(232, 232, 240);
pub const CAR_ROOF: Color = rgb(205, 205, 215);
pub const CAR_GLASS: Color = rgb(24, 30, 44);
pub const CAR_SHADOW: Color = rgba(0, 0, 0, 110);
pub const TYRE: Color = rgb(12, 12, 14);
pub const BRAKE_LIGHT_OFF: Color = rgb(90, 18, 24);
pub const BRAKE_LIGHT_ON: Color = rgb(255, 40, 60);

pub const SMOKE: Color = rgb(226, 224, 230);
pub const SKID: Color = rgb(6, 6, 8);

pub const TEXT: Color = rgb(230, 230, 236);
pub const TEXT_DIM: Color = rgb(140, 144, 160);
pub const PANEL: Color = rgba(10, 12, 18, 200);
pub const NEON_PINK: Color = rgb(255, 46, 136);
pub const NEON_CYAN: Color = rgb(0, 229, 255);
pub const SODIUM: Color = rgb(255, 170, 60);

pub const GRIP: Color = rgb(80, 220, 120);
pub const SLIDING: Color = rgb(255, 170, 60);
pub const SATURATED: Color = rgb(255, 60, 80);

/// Colour for a tyre's combined slip: green gripping, orange at the limit,
/// red well past it.
pub fn slip_color(combined_slip: f64) -> Color {
    if combined_slip < 0.9 {
        GRIP
    } else if combined_slip < 2.0 {
        SLIDING
    } else {
        SATURATED
    }
}
