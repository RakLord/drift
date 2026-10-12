/// What a tyre is standing on, as multipliers on its asphalt behaviour.
/// The world decides this per wheel; the car never knows about tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub grip: f64,
    pub rolling_resistance: f64,
}

impl Ground {
    pub const ASPHALT: Self = Self { grip: 1.0, rolling_resistance: 1.0 };
}

impl Default for Ground {
    fn default() -> Self {
        Self::ASPHALT
    }
}
