//! 2D math used by the sim and viewer.
//!
//! The sim works in `f64` (SI units: metres, seconds, radians). Coordinates are
//! right-handed: +x right, +y up, angles counter-clockwise from +x. In a car's
//! local frame, +x is forward and +y is left.

mod angle;
mod scalar;
mod vec2;

pub use angle::{angle_diff, deg_to_rad, rad_to_deg, wrap_angle};
pub use scalar::{approach, inverse_lerp, lerp, smoothstep};
pub use vec2::Vec2;
