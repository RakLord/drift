//! Sawing: a pedal or the steering going back and forth quickly. A single
//! swing (a lift, a counter-steer catch, a transition) is fine; a reversal
//! that comes hard on the heels of the one before is a saw.

use serde::{Deserialize, Serialize};

/// What counts as a saw on one control.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SawParams {
    /// A change of direction counts once the control has come back this far
    /// from its turning point (control units: 0..1 pedal, -1..1 steer)...
    pub reversal: f64,
    /// ...and it's a saw if the reversal before was within this long, s.
    pub window_secs: f64,
}

/// Saw settings for the controls training watches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmoothnessConfig {
    pub throttle: SawParams,
    pub steer: SawParams,
}

impl Default for SmoothnessConfig {
    fn default() -> Self {
        Self {
            throttle: SawParams { reversal: 0.2, window_secs: 0.3 },
            steer: SawParams { reversal: 0.3, window_secs: 0.3 },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Heading {
    Unknown { low: f64, high: f64 },
    Rising { peak: f64 },
    Falling { trough: f64 },
}

/// Follows one control, tick by tick, and spots saws.
#[derive(Clone, Copy, Debug)]
pub struct SawDetector {
    heading: Option<Heading>,
    last_reversal: f64,
}

impl Default for SawDetector {
    fn default() -> Self {
        Self { heading: None, last_reversal: f64::NEG_INFINITY }
    }
}

impl SawDetector {
    /// Feed in the control's value at `time`. Returns true if it just sawed.
    pub fn update(&mut self, p: &SawParams, value: f64, time: f64) -> bool {
        let heading = self.heading.unwrap_or(Heading::Unknown { low: value, high: value });
        let (next, reversed) = match heading {
            Heading::Unknown { low, high } => {
                let (low, high) = (low.min(value), high.max(value));
                if value >= low + p.reversal {
                    (Heading::Rising { peak: value }, false)
                } else if value <= high - p.reversal {
                    (Heading::Falling { trough: value }, false)
                } else {
                    (Heading::Unknown { low, high }, false)
                }
            }
            Heading::Rising { peak } if value <= peak - p.reversal => (Heading::Falling { trough: value }, true),
            Heading::Rising { peak } => (Heading::Rising { peak: peak.max(value) }, false),
            Heading::Falling { trough } if value >= trough + p.reversal => (Heading::Rising { peak: value }, true),
            Heading::Falling { trough } => (Heading::Falling { trough: trough.min(value) }, false),
        };
        self.heading = Some(next);
        if !reversed {
            return false;
        }
        let saw = time - self.last_reversal <= p.window_secs;
        self.last_reversal = time;
        saw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 120.0;
    const STEER: SawParams = SawParams { reversal: 0.3, window_secs: 0.3 };

    /// Saws over a scripted control trace `f(time)` lasting `secs`.
    fn saws(f: impl Fn(f64) -> f64, secs: f64) -> u32 {
        let mut d = SawDetector::default();
        let ticks = (secs / DT) as usize;
        (0..ticks).filter(|&i| d.update(&STEER, f(i as f64 * DT), i as f64 * DT)).count() as u32
    }

    #[test]
    fn a_counter_steer_catch_is_not_a_saw() {
        // Turn in, flick to opposite lock to catch the slide, hold it.
        let catch = |t: f64| if t < 1.0 { 0.6 } else if t < 1.2 { 0.6 - 6.0 * (t - 1.0) } else { -0.6 };
        assert_eq!(saws(catch, 4.0), 0);
    }

    #[test]
    fn transitions_a_second_and_a_half_apart_are_not_saws() {
        let transitions = |t: f64| if ((t / 1.5) as u32).is_multiple_of(2) { 0.7 } else { -0.7 };
        assert_eq!(saws(transitions, 9.0), 0);
    }

    #[test]
    fn ten_hertz_sawing_counts() {
        // The steer target slamming side to side ten times a second.
        let saw = |t: f64| if ((t * 20.0) as u32).is_multiple_of(2) { 0.5 } else { -0.5 };
        let n = saws(saw, 3.0);
        assert!(n >= 55, "{n} saws in 3 s");
    }

    #[test]
    fn small_wobbles_are_below_the_reversal() {
        let wobble = |t: f64| 0.4 + 0.1 * (t * 40.0).sin();
        assert_eq!(saws(wobble, 3.0), 0);
    }
}
