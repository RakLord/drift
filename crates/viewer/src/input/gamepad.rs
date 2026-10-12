//! Gamepad driving via gilrs. Uses standard (SDL-style) mappings, so most
//! Xbox / PlayStation style pads work, including generic clones.
//!
//! Layout: left stick steers, RT throttle, LT brake, A / Cross or RB
//! handbrake, X / Square or LB clutch, Start resets the car, Select / Back
//! restarts a stalled engine.

use gilrs::{Axis, Button, GamepadId, Gilrs};
use sim::Controls;

const STICK_DEADZONE: f32 = 0.06;
const TRIGGER_DEADZONE: f32 = 0.03;

pub struct Gamepad {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
    reset_pressed: bool,
    restart_pressed: bool,
}

impl Gamepad {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(err) => {
                eprintln!("gamepad support unavailable: {err}");
                None
            }
        };
        let active = gilrs.as_ref().and_then(|g| g.gamepads().next().map(|(id, _)| id));
        Self { gilrs, active, reset_pressed: false, restart_pressed: false }
    }

    /// Process pending events. Must run once per frame.
    pub fn poll(&mut self) {
        self.reset_pressed = false;
        self.restart_pressed = false;
        let Some(gilrs) = &mut self.gilrs else { return };
        while let Some(event) = gilrs.next_event() {
            // Whichever pad was touched last becomes the active one.
            self.active = Some(event.id);
            match event.event {
                gilrs::EventType::ButtonPressed(Button::Start, _) => self.reset_pressed = true,
                gilrs::EventType::ButtonPressed(Button::Select, _) => self.restart_pressed = true,
                _ => {}
            }
        }
    }

    pub fn reset_pressed(&self) -> bool {
        self.reset_pressed
    }

    /// Select / Back: restart a stalled engine.
    pub fn restart_pressed(&self) -> bool {
        self.restart_pressed
    }

    pub fn controls(&self) -> Option<Controls> {
        let pad = self.gilrs.as_ref()?.connected_gamepad(self.active?)?;

        let stick = pad.value(Axis::LeftStickX);
        let steer = if stick.abs() < STICK_DEADZONE {
            0.0
        } else {
            (stick.abs() - STICK_DEADZONE) / (1.0 - STICK_DEADZONE) * stick.signum()
        };

        // Triggers show up as analog buttons on most pads, as Z axes on some.
        let trigger = |button: Button, axis: Axis| {
            let value = pad.button_data(button).map_or(0.0, |b| b.value()).max(pad.value(axis).max(0.0));
            if value < TRIGGER_DEADZONE { 0.0 } else { f64::from(value) }
        };
        let held = |buttons: &[Button]| if buttons.iter().any(|&b| pad.is_pressed(b)) { 1.0 } else { 0.0 };

        Some(Controls {
            // Stick right is +x, but positive steer turns left.
            steer: -f64::from(steer),
            throttle: trigger(Button::RightTrigger2, Axis::RightZ),
            brake: trigger(Button::LeftTrigger2, Axis::LeftZ),
            handbrake: held(&[Button::South, Button::RightTrigger]),
            clutch: held(&[Button::West, Button::LeftTrigger]),
        })
    }

    /// Raw readout for identifying an unknown controller.
    pub fn debug_lines(&self) -> Vec<String> {
        let Some(gilrs) = &self.gilrs else { return vec!["gamepad support unavailable".to_owned()] };
        let mut lines = vec![];
        for (id, pad) in gilrs.gamepads() {
            let marker = if Some(id) == self.active { ">" } else { " " };
            lines.push(format!("{marker} {} ({:?} mapping)", pad.name(), pad.mapping_source()));
        }
        if lines.is_empty() {
            lines.push("no gamepad connected".to_owned());
        }
        let Some(pad) = self.active.and_then(|id| gilrs.connected_gamepad(id)) else { return lines };

        let axes = [
            Axis::LeftStickX,
            Axis::LeftStickY,
            Axis::RightStickX,
            Axis::RightStickY,
            Axis::LeftZ,
            Axis::RightZ,
        ];
        for axis in axes {
            lines.push(format!("  {axis:?}: {:+.2}", pad.value(axis)));
        }
        let buttons = [
            Button::South,
            Button::East,
            Button::West,
            Button::North,
            Button::LeftTrigger,
            Button::RightTrigger,
            Button::LeftTrigger2,
            Button::RightTrigger2,
            Button::Start,
            Button::Select,
        ];
        for button in buttons {
            let value = pad.button_data(button).map_or(0.0, |b| b.value());
            lines.push(format!("  {button:?}: {value:.2}"));
        }
        lines
    }
}
