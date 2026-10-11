use serde::{Deserialize, Serialize};

/// What a driver (human, network or script) asks the car to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    /// Target steering, -1 (full right) to 1 (full left). Positive turns left,
    /// matching the counter-clockwise angle convention.
    pub steer: f64,
    pub throttle: f64,
    pub brake: f64,
    pub handbrake: f64,
    /// Clutch pedal: 0 = engaged, 1 = fully disengaged.
    pub clutch: f64,
}

impl Controls {
    /// Clamp every field into its valid range and replace NaN with 0.
    pub fn clamped(self) -> Self {
        let unit = |v: f64| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        Self {
            steer: if self.steer.is_nan() { 0.0 } else { self.steer.clamp(-1.0, 1.0) },
            throttle: unit(self.throttle),
            brake: unit(self.brake),
            handbrake: unit(self.handbrake),
            clutch: unit(self.clutch),
        }
    }
}
