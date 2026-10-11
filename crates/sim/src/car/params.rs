//! Car parameters. The single source of truth is `config/car.toml`; the
//! default here is that file baked in at compile time.

use common::Vec2;
use serde::{Deserialize, Serialize};

use super::wheel::{FL, FR, RL, RR};
use crate::GRAVITY;

const DEFAULT_CAR_TOML: &str = include_str!("../../../../config/car.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarParams {
    pub substeps: u32,
    pub body: BodyParams,
    pub suspension: SuspensionParams,
    pub steering: SteeringParams,
    pub wheel: WheelParams,
    pub tyre: TyreParams,
    pub engine: EngineParams,
    pub clutch: ClutchParams,
    pub drivetrain: DrivetrainParams,
    pub brakes: BrakeParams,
    pub aero: AeroParams,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyParams {
    pub mass: f64,
    pub yaw_inertia: f64,
    pub wheelbase: f64,
    pub cg_to_front_axle: f64,
    pub track_width_front: f64,
    pub track_width_rear: f64,
    pub cg_height: f64,
    pub length: f64,
    pub width: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuspensionParams {
    pub roll_frequency_hz: f64,
    pub roll_damping_ratio: f64,
    pub pitch_frequency_hz: f64,
    pub pitch_damping_ratio: f64,
    pub front_roll_share: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringParams {
    pub max_lock_deg: f64,
    pub rate_deg_per_s: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelParams {
    pub radius: f64,
    pub inertia: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TyreParams {
    pub mu_peak: f64,
    pub slide_ratio: f64,
    pub peak_slip_long: f64,
    pub peak_slip_lat: f64,
    pub falloff: f64,
    pub load_sensitivity: f64,
    pub reference_load: f64,
    pub min_speed: f64,
    pub rolling_resistance: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineParams {
    pub inertia: f64,
    pub idle_rpm: f64,
    pub idle_governor_band_rpm: f64,
    pub rev_limit_rpm: f64,
    pub limiter_hysteresis_rpm: f64,
    pub friction_base: f64,
    pub friction_per_krpm: f64,
    /// `[rpm, torque Nm]` pairs, sorted by rpm.
    pub torque_curve: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClutchParams {
    pub max_torque: f64,
    pub stall_rpm: f64,
    pub bite_rpm: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrivetrainParams {
    pub ratio: f64,
    pub inertia: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrakeParams {
    pub front_torque: f64,
    pub rear_torque: f64,
    pub handbrake_torque: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AeroParams {
    pub drag: f64,
}

impl Default for CarParams {
    fn default() -> Self {
        toml::from_str(DEFAULT_CAR_TOML).expect("config/car.toml is invalid")
    }
}

impl BodyParams {
    pub fn cg_to_rear_axle(&self) -> f64 {
        self.wheelbase - self.cg_to_front_axle
    }

    /// Contact patch positions relative to the CG, in the car frame
    /// (x forward, y left), indexed FL, FR, RL, RR.
    pub fn wheel_offsets(&self) -> [Vec2; 4] {
        let mut offsets = [Vec2::ZERO; 4];
        let front = self.cg_to_front_axle;
        let rear = -self.cg_to_rear_axle();
        offsets[FL] = Vec2::new(front, self.track_width_front / 2.0);
        offsets[FR] = Vec2::new(front, -self.track_width_front / 2.0);
        offsets[RL] = Vec2::new(rear, self.track_width_rear / 2.0);
        offsets[RR] = Vec2::new(rear, -self.track_width_rear / 2.0);
        offsets
    }

    /// Per-wheel load with the car at rest, indexed FL, FR, RL, RR.
    pub fn static_loads(&self) -> [f64; 4] {
        let weight = self.mass * GRAVITY;
        let front_each = weight * self.cg_to_rear_axle() / self.wheelbase / 2.0;
        let rear_each = weight * self.cg_to_front_axle / self.wheelbase / 2.0;
        [front_each, front_each, rear_each, rear_each]
    }
}

impl SteeringParams {
    pub fn max_lock(&self) -> f64 {
        self.max_lock_deg.to_radians()
    }

    pub fn rate(&self) -> f64 {
        self.rate_deg_per_s.to_radians()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let params = CarParams::default();
        assert!(params.body.mass > 0.0);
        assert!(params.engine.torque_curve.windows(2).all(|w| w[0][0] < w[1][0]));
    }

    #[test]
    fn static_loads_sum_to_weight() {
        let body = CarParams::default().body;
        let total: f64 = body.static_loads().iter().sum();
        assert!((total - body.mass * GRAVITY).abs() < 1e-6);
    }
}
