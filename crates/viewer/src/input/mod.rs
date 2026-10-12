//! Turns gamepad / keyboard state into car `Controls`.

pub mod ai;
pub mod autopilot;
pub mod gamepad;
pub mod keyboard;

use sim::Controls;

use gamepad::Gamepad;
use keyboard::Keyboard;

pub struct Input {
    pub gamepad: Gamepad,
    keyboard: Keyboard,
}

impl Input {
    pub fn new() -> Self {
        Self { gamepad: Gamepad::new(), keyboard: Keyboard::default() }
    }

    /// Controls for this frame. The keyboard wins while any of its driving
    /// keys are held; otherwise the gamepad (if connected) drives.
    pub fn controls(&mut self, dt: f64) -> Controls {
        self.gamepad.poll();
        let keyboard = self.keyboard.update(dt);
        if self.keyboard.active() {
            return keyboard;
        }
        self.gamepad.controls().unwrap_or(keyboard)
    }

    pub fn reset_pressed(&self) -> bool {
        self.gamepad.reset_pressed()
    }
}
