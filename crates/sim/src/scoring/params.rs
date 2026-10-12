//! Scoring parameters. Source of truth: `config/scoring.toml`, baked in as
//! the default.

use serde::{Deserialize, Serialize};

const DEFAULT_SCORING_TOML: &str = include_str!("../../../../config/scoring.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringParams {
    pub drift: DriftParams,
    pub angle: AngleParams,
    pub speed: SpeedParams,
    pub line: LineParams,
    pub style: StyleParams,
    pub combo: ComboParams,
    /// Missing in older replays and top files: the defaults then.
    #[serde(default)]
    pub flow: FlowParams,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriftParams {
    pub start_angle_deg: f64,
    pub min_speed_kmh: f64,
    pub end_angle_deg: f64,
    pub end_hold_secs: f64,
    pub spin_angle_deg: f64,
    /// Switching sides only counts as a transition if the side before was
    /// held this long; quicker is a flick (0 in older files: every switch).
    #[serde(default)]
    pub transition_min_hold_secs: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AngleParams {
    pub points_per_sec: f64,
    pub full_from_deg: f64,
    pub full_to_deg: f64,
    pub zero_at_deg: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedParams {
    pub slow_kmh: f64,
    pub slow_multiplier: f64,
    pub fast_kmh: f64,
    pub fast_multiplier: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineParams {
    pub clip_points: f64,
    pub clip_full_within: f64,
    pub clip_zero_beyond: f64,
    pub clip_window_before: f64,
    pub clip_window_after: f64,
    pub outer_points_per_sec: f64,
    pub outer_full_within: f64,
    pub outer_zero_beyond: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleParams {
    pub transition_points: f64,
    pub wall_tap_points: f64,
    pub wall_tap_min_speed: f64,
    pub wall_tap_max_speed: f64,
    pub wall_tap_cooldown_secs: f64,
    pub max_taps_per_combo: u32,
    pub bobble_drop_deg: f64,
    pub bobble_window_secs: f64,
    pub bobble_penalty: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComboParams {
    pub multiplier_step: f64,
    pub multiplier_max: f64,
    pub spin_penalty: f64,
    pub hard_hit_speed: f64,
    pub hard_hit_penalty: f64,
}

/// Flow: drift credit that builds up the longer one side is held (see
/// `super::flow`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowParams {
    /// Hold credit ramps from 0 to 1 over this many seconds on one side...
    pub ramp_secs: f64,
    /// ...then on up to 1 + bonus...
    pub bonus: f64,
    /// ...by this many seconds held.
    pub bonus_secs: f64,
    /// Share of the ramp carried into the next side by a linked transition.
    pub link_keep: f64,
}

impl Default for FlowParams {
    fn default() -> Self {
        ScoringParams::default().flow
    }
}

impl FlowParams {
    /// Credit multiplier after `hold` seconds on one side.
    pub fn hold_credit(&self, hold: f64) -> f64 {
        if hold < self.ramp_secs {
            hold / self.ramp_secs
        } else {
            let t = common::math::inverse_lerp(self.ramp_secs, self.bonus_secs, hold).clamp(0.0, 1.0);
            1.0 + self.bonus * t
        }
    }
}

impl Default for ScoringParams {
    fn default() -> Self {
        toml::from_str(DEFAULT_SCORING_TOML).expect("config/scoring.toml is invalid")
    }
}

impl AngleParams {
    /// 0..1 share of angle points at a drift angle of `deg` (absolute):
    /// ramps up from the drift start angle, full in the sweet spot, tapers
    /// towards a spin.
    pub fn factor(&self, deg: f64, start_deg: f64) -> f64 {
        if deg < start_deg {
            0.0
        } else if deg < self.full_from_deg {
            common::math::smoothstep(start_deg, self.full_from_deg, deg)
        } else if deg <= self.full_to_deg {
            1.0
        } else {
            1.0 - common::math::smoothstep(self.full_to_deg, self.zero_at_deg, deg)
        }
    }
}

impl SpeedParams {
    pub fn multiplier(&self, kmh: f64) -> f64 {
        let t = common::math::inverse_lerp(self.slow_kmh, self.fast_kmh, kmh).clamp(0.0, 1.0);
        common::math::lerp(self.slow_multiplier, self.fast_multiplier, t)
    }
}

/// 1 within `full`, 0 beyond `zero`, linear between.
pub fn proximity(distance: f64, full: f64, zero: f64) -> f64 {
    ((zero - distance) / (zero - full)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let p = ScoringParams::default();
        assert!(p.combo.multiplier_max >= 1.0);
    }

    #[test]
    fn angle_factor_shape() {
        let p = ScoringParams::default();
        let start = p.drift.start_angle_deg;
        let f = |deg| p.angle.factor(deg, start);
        assert_eq!(f(5.0), 0.0);
        assert!(f(20.0) > 0.0 && f(20.0) < 1.0);
        assert_eq!(f(50.0), 1.0);
        assert!(f(85.0) < 1.0);
        assert_eq!(f(p.angle.zero_at_deg), 0.0);
    }

    #[test]
    fn hold_credit_ramps_then_bonus() {
        let p = ScoringParams::default().flow;
        assert_eq!(p.hold_credit(0.0), 0.0);
        assert!((p.hold_credit(p.ramp_secs / 2.0) - 0.5).abs() < 1e-9);
        assert!((p.hold_credit(p.ramp_secs) - 1.0).abs() < 1e-9);
        assert!((p.hold_credit(p.bonus_secs) - (1.0 + p.bonus)).abs() < 1e-9);
        assert_eq!(p.hold_credit(p.bonus_secs * 10.0), 1.0 + p.bonus);
    }

    #[test]
    fn older_files_without_flow_still_parse() {
        let mut old: toml::Table = toml::from_str(DEFAULT_SCORING_TOML).unwrap();
        old.remove("flow");
        old["drift"].as_table_mut().unwrap().remove("transition_min_hold_secs");
        let p: ScoringParams = toml::from_str(&toml::to_string(&old).unwrap()).unwrap();
        assert_eq!(p.drift.transition_min_hold_secs, 0.0, "every switch is a transition, as before");
        assert_eq!(p.flow.ramp_secs, ScoringParams::default().flow.ramp_secs);
    }

    #[test]
    fn speed_multiplier_clamps() {
        let p = ScoringParams::default().speed;
        assert_eq!(p.multiplier(0.0), p.slow_multiplier);
        assert_eq!(p.multiplier(500.0), p.fast_multiplier);
    }
}
