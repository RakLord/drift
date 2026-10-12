//! Points held in a combo until the drift ends cleanly.

use std::ops::{Add, AddAssign};

use super::params::ComboParams;

/// Points split by judging category. Penalties are kept separate and are
/// never multiplied.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Breakdown {
    pub line: f64,
    pub angle: f64,
    pub style: f64,
    pub penalty: f64,
}

impl Breakdown {
    pub fn total(&self) -> f64 {
        self.line + self.angle + self.style - self.penalty
    }
}

impl Add for Breakdown {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self {
            line: self.line + o.line,
            angle: self.angle + o.angle,
            style: self.style + o.style,
            penalty: self.penalty + o.penalty,
        }
    }
}

impl AddAssign for Breakdown {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

#[derive(Clone, Debug)]
pub struct Combo {
    pub active: bool,
    pub pending: Breakdown,
    pub multiplier: f64,
    /// Wall taps credited in this combo.
    pub taps: u32,
}

impl Default for Combo {
    fn default() -> Self {
        Self { active: false, pending: Breakdown::default(), multiplier: 1.0, taps: 0 }
    }
}

impl Combo {
    pub fn start(&mut self) {
        if !self.active {
            *self = Self { active: true, ..Self::default() };
        }
    }

    /// One step up the multiplier (zone, transition or credited tap).
    pub fn bump(&mut self, p: &ComboParams) {
        self.multiplier = (self.multiplier + p.multiplier_step).min(p.multiplier_max);
    }

    /// Value of the combo if banked now.
    pub fn value(&self) -> Breakdown {
        let m = self.multiplier;
        Breakdown {
            line: self.pending.line * m,
            angle: self.pending.angle * m,
            style: self.pending.style * m,
            penalty: self.pending.penalty,
        }
    }

    /// End cleanly: returns what goes into the score.
    pub fn bank(&mut self) -> Breakdown {
        let value = self.value();
        *self = Self::default();
        value
    }

    /// End badly: everything pending is gone.
    pub fn lose(&mut self) {
        *self = Self::default();
    }
}
