//! Drift detection: when a drift starts, links into the other direction,
//! ends cleanly, or turns into a spin.

use super::params::DriftParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriftEvent {
    None,
    Started,
    /// Direction changed without the drift ending (linked), after holding
    /// the side before for at least `transition_min_hold_secs`.
    Transition,
    /// Direction changed, but the side before was too short: a flick.
    Flick,
    /// Angle stayed low long enough, or the car slowed or went backwards.
    Ended,
    Spin,
}

#[derive(Clone, Debug, Default)]
pub struct DriftDetector {
    pub active: bool,
    /// Sign of the drift angle for the current drift.
    pub direction: f64,
    /// Seconds on the current side (since the start or the last switch).
    pub side_secs: f64,
    below_for: f64,
    /// Spun and not yet straightened out; no new drift can start.
    spun: bool,
}

impl DriftDetector {
    /// End the drift badly (e.g. a crash). Like a spin, no new drift can
    /// start until the car has straightened out.
    pub fn interrupt(&mut self) {
        self.active = false;
        self.spun = true;
    }

    /// `angle` is the signed drift angle (radians), `speed` m/s, `forward`
    /// whether the car is travelling the right way along the track.
    pub fn update(&mut self, p: &DriftParams, angle: f64, speed: f64, forward: bool, dt: f64) -> DriftEvent {
        let deg = angle.abs().to_degrees();
        let kmh = speed * 3.6;

        if deg > p.spin_angle_deg && kmh > 5.0 && !self.spun {
            self.active = false;
            self.spun = true;
            return DriftEvent::Spin;
        }
        if self.spun {
            if deg < p.end_angle_deg || kmh < 3.0 {
                self.spun = false;
            }
            return DriftEvent::None;
        }

        if !self.active {
            if deg >= p.start_angle_deg && kmh >= p.min_speed_kmh && forward {
                self.active = true;
                self.direction = angle.signum();
                self.side_secs = 0.0;
                self.below_for = 0.0;
                return DriftEvent::Started;
            }
            return DriftEvent::None;
        }

        self.side_secs += dt;
        if !forward || kmh < p.min_speed_kmh * 0.5 {
            self.active = false;
            return DriftEvent::Ended;
        }
        if deg < p.end_angle_deg {
            self.below_for += dt;
            if self.below_for >= p.end_hold_secs {
                self.active = false;
                return DriftEvent::Ended;
            }
            return DriftEvent::None;
        }
        self.below_for = 0.0;
        if deg >= p.start_angle_deg && angle.signum() != self.direction {
            let held = self.side_secs >= p.transition_min_hold_secs;
            self.direction = angle.signum();
            self.side_secs = 0.0;
            return if held { DriftEvent::Transition } else { DriftEvent::Flick };
        }
        DriftEvent::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::ScoringParams;

    const DT: f64 = 1.0 / 120.0;

    fn run(d: &mut DriftDetector, deg: f64, secs: f64) -> Vec<DriftEvent> {
        let p = ScoringParams::default().drift;
        let ticks = (secs / DT).round() as usize;
        (0..ticks)
            .map(|_| d.update(&p, deg.to_radians(), 15.0, true, DT))
            .filter(|e| *e != DriftEvent::None)
            .collect()
    }

    #[test]
    fn start_hold_and_clean_exit() {
        let mut d = DriftDetector::default();
        assert_eq!(run(&mut d, 30.0, 1.0), vec![DriftEvent::Started]);
        assert_eq!(run(&mut d, 3.0, 0.1), vec![]);
        assert!(d.active, "a brief dip must not end the drift");
        assert_eq!(run(&mut d, 3.0, 0.5), vec![DriftEvent::Ended]);
    }

    #[test]
    fn held_side_then_quick_switch_is_a_transition() {
        let mut d = DriftDetector::default();
        run(&mut d, 30.0, 1.0);
        assert_eq!(run(&mut d, 2.0, 0.1), vec![]);
        assert_eq!(run(&mut d, -30.0, 0.5), vec![DriftEvent::Transition]);
    }

    #[test]
    fn short_side_then_switch_is_a_flick() {
        let mut d = DriftDetector::default();
        run(&mut d, 30.0, 0.3);
        assert_eq!(run(&mut d, -30.0, 0.3), vec![DriftEvent::Flick]);
        assert_eq!(run(&mut d, 30.0, 1.0), vec![DriftEvent::Flick], "0.3 s on the left side is still a flick");
        assert_eq!(run(&mut d, -30.0, 0.3), vec![DriftEvent::Transition]);
    }

    #[test]
    fn spin_reports_once_until_straight() {
        let mut d = DriftDetector::default();
        run(&mut d, 40.0, 0.5);
        assert_eq!(run(&mut d, 140.0, 1.0), vec![DriftEvent::Spin]);
        assert_eq!(run(&mut d, 40.0, 0.5), vec![], "can't start drifting mid-spin");
        run(&mut d, 2.0, 0.1);
        assert_eq!(run(&mut d, 30.0, 0.2), vec![DriftEvent::Started]);
    }
}
