//! Flow: how smoothly the car drifts, for training's drift quality. Each
//! second of drifting earns the angle points' shape (full at a big but
//! controlled angle) times a hold credit that builds up the longer one side
//! is held. A linked transition carries most of it into the next side; a
//! flick, an end or a spin starts it again from nothing.

use super::drift::DriftEvent;
use super::params::ScoringParams;

#[derive(Clone, Debug, Default)]
pub struct Flow {
    /// Seconds of hold credited on the current side.
    pub hold: f64,
    /// Credit per second this tick.
    pub credit: f64,
    /// Sides drifted so far: starts, transitions and flicks.
    pub sides: u32,
}

impl Flow {
    /// `event` is this tick's drift event, `active` whether the car is
    /// drifting after it, `deg` the absolute drift angle.
    pub fn update(&mut self, p: &ScoringParams, event: DriftEvent, active: bool, deg: f64, on_verge: bool, dt: f64) {
        match event {
            DriftEvent::Started => self.sides += 1,
            DriftEvent::Transition => {
                self.hold = p.flow.link_keep * self.hold.min(p.flow.ramp_secs);
                self.sides += 1;
            }
            DriftEvent::Flick => {
                self.hold = 0.0;
                self.sides += 1;
            }
            DriftEvent::Ended | DriftEvent::Spin | DriftEvent::None => {}
        }
        if !active {
            self.hold = 0.0;
            self.credit = 0.0;
            return;
        }
        self.hold += dt;
        self.credit = if on_verge {
            0.0
        } else {
            p.angle.factor(deg, p.drift.start_angle_deg) * p.flow.hold_credit(self.hold)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::drift::DriftDetector;

    const DT: f64 = 1.0 / 120.0;

    /// Drift at `deg` (signed) for `secs`; returns the credit earned.
    fn drive(d: &mut DriftDetector, f: &mut Flow, p: &ScoringParams, deg: f64, secs: f64, on_verge: bool) -> f64 {
        let ticks = (secs / DT).round() as usize;
        let mut earned = 0.0;
        for _ in 0..ticks {
            let event = d.update(&p.drift, deg.to_radians(), 15.0, true, DT);
            f.update(p, event, d.active, deg.abs(), on_verge, DT);
            earned += f.credit * DT;
        }
        earned
    }

    #[test]
    fn credit_builds_up_while_held() {
        let p = ScoringParams::default();
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        let first = drive(&mut d, &mut f, &p, 50.0, 0.5, false);
        let second = drive(&mut d, &mut f, &p, 50.0, 0.5, false);
        assert!(second > 2.0 * first);
        drive(&mut d, &mut f, &p, 50.0, p.flow.bonus_secs, false);
        assert!((f.credit - (1.0 + p.flow.bonus)).abs() < 1e-9, "full angle, full bonus");
    }

    #[test]
    fn a_long_arc_beats_the_same_time_in_flicks() {
        let p = ScoringParams::default();
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        let arc = drive(&mut d, &mut f, &p, 45.0, 3.0, false);
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        let flicks: f64 = (0..10).map(|i| drive(&mut d, &mut f, &p, if i % 2 == 0 { 45.0 } else { -45.0 }, 0.3, false)).sum();
        assert_eq!(f.sides, 10);
        assert!(arc > 4.0 * flicks, "arc {arc} vs flicks {flicks}");
    }

    #[test]
    fn linked_transition_keeps_most_of_the_ramp() {
        let p = ScoringParams::default();
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        drive(&mut d, &mut f, &p, 45.0, 2.0, false);
        drive(&mut d, &mut f, &p, -45.0, DT, false);
        assert!((f.hold - p.flow.link_keep * p.flow.ramp_secs - DT).abs() < 1e-9);
        assert_eq!(f.sides, 2);
    }

    #[test]
    fn flicks_and_ends_start_again() {
        let p = ScoringParams::default();
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        drive(&mut d, &mut f, &p, 45.0, 0.3, false);
        drive(&mut d, &mut f, &p, -45.0, DT, false);
        assert!((f.hold - DT).abs() < 1e-9, "a flick starts the ramp again");
        drive(&mut d, &mut f, &p, -45.0, 2.0, false);
        drive(&mut d, &mut f, &p, 2.0, 1.0, false);
        assert!(!d.active);
        assert_eq!((f.hold, f.credit), (0.0, 0.0));
    }

    #[test]
    fn nothing_on_the_verge_but_the_hold_keeps_counting() {
        let p = ScoringParams::default();
        let (mut d, mut f) = (DriftDetector::default(), Flow::default());
        assert_eq!(drive(&mut d, &mut f, &p, 45.0, 1.0, true), 0.0);
        drive(&mut d, &mut f, &p, 45.0, DT, false);
        assert!(f.credit >= 1.0);
    }
}
