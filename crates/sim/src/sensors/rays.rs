//! Raycasts from the car to the track edges: the car's "eyes".

use common::Vec2;

use crate::track::Track;

pub const RAY_COUNT: usize = FRONT_RAYS + REAR_RAYS;
/// Rays across the front half (denser: that's where the car is going).
const FRONT_RAYS: usize = 32;
const REAR_RAYS: usize = 16;
/// Rays see this far, metres.
pub const RAY_RANGE: f64 = 60.0;

/// Ray angles relative to the car's heading, radians: 32 spread across the
/// front 180 degrees, then 16 across the rear 180, going anticlockwise.
pub fn angles() -> [f64; RAY_COUNT] {
    use std::f64::consts::PI;
    std::array::from_fn(|i| {
        if i < FRONT_RAYS {
            -PI / 2.0 + (i as f64 + 0.5) * PI / FRONT_RAYS as f64
        } else {
            PI / 2.0 + ((i - FRONT_RAYS) as f64 + 0.5) * PI / REAR_RAYS as f64
        }
    })
}

/// Distance to the nearest track edge along each ray (`RAY_RANGE` if none).
pub fn cast(track: &Track, origin: Vec2, heading: f64, angles: &[f64; RAY_COUNT]) -> [f64; RAY_COUNT] {
    std::array::from_fn(|i| {
        let direction = Vec2::from_angle(heading + angles[i]);
        track.raycast(origin, direction, RAY_RANGE).map_or(RAY_RANGE, |hit| hit.distance)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_cover_front_densely_and_rear_sparsely() {
        let a = angles();
        let front = a.iter().filter(|x| x.abs() < std::f64::consts::FRAC_PI_2).count();
        assert_eq!(front, FRONT_RAYS);
        assert!(a.windows(2).all(|w| w[1] > w[0]), "rays go anticlockwise");
    }
}
