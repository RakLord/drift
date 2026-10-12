//! Why generated attempts get thrown away, so a failed seed can say what to
//! change rather than just "no track".

use std::fmt;

use crate::track::TrackError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    NoClosingPath,
    ClosingStraight,
    ClosingTurn,
    TooShort,
    TooLong,
    TooFewCrossings,
    TooManyCrossings,
    CornerTooTight,
    BoundariesCross,
    ShallowCrossing,
    RoadsTooClose,
    NoZones,
    Other,
}

impl Rejection {
    const ALL: [Self; 13] = [
        Self::NoClosingPath,
        Self::ClosingStraight,
        Self::ClosingTurn,
        Self::TooShort,
        Self::TooLong,
        Self::TooFewCrossings,
        Self::TooManyCrossings,
        Self::CornerTooTight,
        Self::BoundariesCross,
        Self::ShallowCrossing,
        Self::RoadsTooClose,
        Self::NoZones,
        Self::Other,
    ];

    pub fn describe(self) -> &'static str {
        match self {
            Self::NoClosingPath => "couldn't find a path back to the start",
            Self::ClosingStraight => "closing straight too long",
            Self::ClosingTurn => "closing turn too big",
            Self::TooShort => "track too short",
            Self::TooLong => "track too long",
            Self::TooFewCrossings => "fewer crossings than 'min crossings'",
            Self::TooManyCrossings => "more crossings than 'max crossings'",
            Self::CornerTooTight => "corner too tight for the track width + verge",
            Self::BoundariesCross => "walls/fences overlap another part of the track",
            Self::ShallowCrossing => "track crosses itself at too shallow an angle",
            Self::RoadsTooClose => "two parts of the track run too close side by side",
            Self::NoZones => "no room for any zones (corners too small or all at crossroads)",
            Self::Other => "other",
        }
    }

    /// Which settings to change to make this rejection rarer.
    pub fn hint(self) -> &'static str {
        match self {
            Self::NoClosingPath => "lower 'closing radius'",
            Self::ClosingStraight => "raise 'max closing straight' or 'homing end'",
            Self::ClosingTurn => "raise 'max closing turn' or 'homing end'",
            Self::TooShort => "lower 'length min' or raise 'pieces'",
            Self::TooLong => "raise 'length max' or lower 'pieces'",
            Self::TooFewCrossings => "raise 'figure-8' or lower 'min crossings'",
            Self::TooManyCrossings => "raise 'max crossings' or lower 'figure-8'",
            Self::CornerTooTight => "raise the piece radius minimums, or lower width / verge",
            Self::BoundariesCross | Self::RoadsTooClose => "lower 'pieces max' or 'homing', or narrow the track / verge",
            Self::ShallowCrossing => "lower 'max crossings' to 0, or raise 'homing'",
            Self::NoZones => "lower the zone min angles, or raise piece angles",
            Self::Other => "check the track settings",
        }
    }

    pub fn from_build(error: &TrackError) -> Self {
        match error {
            TrackError::TooTight { .. } => Self::CornerTooTight,
            TrackError::SelfIntersecting { .. } => Self::BoundariesCross,
            TrackError::ShallowCrossing { .. } => Self::ShallowCrossing,
            TrackError::TooClose { .. } => Self::RoadsTooClose,
            _ => Self::Other,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&r| r == self).unwrap_or(0)
    }
}

/// Every attempt for a seed failed. Holds how often each reason came up.
#[derive(Clone, Debug)]
pub struct GenerationFailure {
    pub seed: u64,
    pub attempts: u32,
    counts: [u32; Rejection::ALL.len()],
}

impl GenerationFailure {
    pub fn new(seed: u64, attempts: u32) -> Self {
        Self { seed, attempts, counts: [0; Rejection::ALL.len()] }
    }

    pub fn record(&mut self, reason: Rejection) {
        self.counts[reason.index()] += 1;
    }

    /// Reasons that came up, most common first.
    pub fn reasons(&self) -> Vec<(Rejection, u32)> {
        let mut reasons: Vec<_> =
            Rejection::ALL.iter().map(|&r| (r, self.counts[r.index()])).filter(|&(_, n)| n > 0).collect();
        reasons.sort_by(|a, b| b.1.cmp(&a.1));
        reasons
    }

    /// Short lines for display: a headline, then the top reasons with hints.
    pub fn lines(&self, max_reasons: usize) -> Vec<String> {
        let mut lines = vec![format!("no valid track for seed {} ({} attempts)", self.seed, self.attempts)];
        for (reason, count) in self.reasons().into_iter().take(max_reasons) {
            let share = 100.0 * f64::from(count) / f64::from(self.attempts.max(1));
            lines.push(format!("{share:>3.0}%  {}  -> {}", reason.describe(), reason.hint()));
        }
        lines
    }
}

impl fmt::Display for GenerationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.lines(3).join("\n"))
    }
}

impl std::error::Error for GenerationFailure {}
