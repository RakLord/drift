//! The corner pieces drift tracks are built from.

use common::Rng;

use super::turtle::Turtle;
use super::{PieceWeights, ShapeParams};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Tight and long: 150-190 degrees.
    Hairpin,
    /// Long, open corner: room for big angle and outer zones.
    Sweeper,
    /// A corner straight into one the other way: forces a transition.
    SBend,
    /// Opens wide then tightens up.
    Decreasing,
    /// Several short flicks alternating direction (manji).
    Chicane,
    /// A small bend taken at speed.
    Kink,
}

impl Piece {
    const ALL: [Self; 6] = [Self::Hairpin, Self::Sweeper, Self::SBend, Self::Decreasing, Self::Chicane, Self::Kink];

    /// Pick a piece in proportion to its weight.
    pub fn choose(weights: &PieceWeights, rng: &mut Rng) -> Self {
        let weight = |p: Self| match p {
            Self::Hairpin => weights.hairpin,
            Self::Sweeper => weights.sweeper,
            Self::SBend => weights.s_bend,
            Self::Decreasing => weights.decreasing,
            Self::Chicane => weights.chicane,
            Self::Kink => weights.kink,
        }
        .max(0.0);
        let total: f64 = Self::ALL.iter().map(|&p| weight(p)).sum();
        if total <= 0.0 {
            return Self::Sweeper;
        }
        let mut pick = rng.range(0.0, total);
        for p in Self::ALL {
            pick -= weight(p);
            if pick < 0.0 {
                return p;
            }
        }
        Self::Kink
    }

    /// Lay this piece, turning first in `direction` (+1 left, -1 right).
    pub fn lay(self, turtle: &mut Turtle, direction: f64, s: &ShapeParams, rng: &mut Rng) {
        let mut range = |[lo, hi]: [f64; 2]| rng.range(lo, hi.max(lo));
        let deg = |d: f64| d.to_radians();
        match self {
            Self::Hairpin => turtle.arc(range(s.hairpin_radius), direction * deg(range(s.hairpin_angle))),
            Self::Sweeper => turtle.arc(range(s.sweeper_radius), direction * deg(range(s.sweeper_angle))),
            Self::Kink => turtle.arc(range(s.kink_radius), direction * deg(range(s.kink_angle))),
            Self::SBend => {
                turtle.arc(range(s.s_bend_radius), direction * deg(range(s.s_bend_angle)));
                let gap = range(s.s_bend_gap);
                if gap > 0.5 {
                    turtle.straight(gap);
                }
                turtle.arc(range(s.s_bend_radius), -direction * deg(range(s.s_bend_angle)));
            }
            Self::Decreasing => {
                turtle.arc(range(s.decreasing_open_radius), direction * deg(range(s.decreasing_angle)));
                turtle.arc(range(s.decreasing_tight_radius), direction * deg(range(s.decreasing_angle)));
            }
            Self::Chicane => {
                let flicks = range(s.chicane_flicks).round().max(2.0) as usize;
                let mut side = direction;
                for _ in 0..flicks {
                    turtle.arc(range(s.chicane_radius), side * deg(range(s.chicane_angle)));
                    side = -side;
                }
            }
        }
    }
}
