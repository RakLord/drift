use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Unit vector pointing at `angle` radians (CCW from +x).
    pub fn from_angle(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::new(cos, sin)
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Z component of the 3D cross product. Positive when `other` is CCW of `self`.
    pub fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    pub fn length_sq(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    pub fn distance(self, other: Self) -> f64 {
        (self - other).length()
    }

    /// Unit vector in the same direction, or zero if the length is (near) zero.
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > 1e-12 { self / len } else { Self::ZERO }
    }

    /// Angle of this vector in radians, in (-PI, PI].
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Rotated 90 degrees counter-clockwise (the "left" normal).
    pub fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    pub fn rotate(self, angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
    }

    /// Express a world-space direction in a frame rotated by `heading`.
    pub fn to_local(self, heading: f64) -> Self {
        self.rotate(-heading)
    }

    /// Express a direction in a frame rotated by `heading` back in world space.
    pub fn to_world(self, heading: f64) -> Self {
        self.rotate(heading)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        self + (other - self) * t
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl Mul<Vec2> for f64 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        rhs * self
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl MulAssign<f64> for Vec2 {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

impl DivAssign<f64> for Vec2 {
    fn div_assign(&mut self, rhs: f64) {
        *self = *self / rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn approx(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn rotate_quarter_turn() {
        assert!(approx(Vec2::X.rotate(FRAC_PI_2), Vec2::Y));
        assert!(approx(Vec2::X.perp(), Vec2::Y));
    }

    #[test]
    fn local_world_round_trip() {
        let v = Vec2::new(3.0, -2.0);
        let heading = 1.234;
        assert!(approx(v.to_local(heading).to_world(heading), v));
    }

    #[test]
    fn to_local_puts_heading_on_x() {
        let heading = 0.7;
        assert!(approx(Vec2::from_angle(heading).to_local(heading), Vec2::X));
    }

    #[test]
    fn cross_sign() {
        assert!(Vec2::X.cross(Vec2::Y) > 0.0);
        assert!(Vec2::Y.cross(Vec2::X) < 0.0);
    }

    #[test]
    fn normalize_zero_is_safe() {
        assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);
        assert!((Vec2::new(3.0, 4.0).normalize_or_zero().length() - 1.0).abs() < 1e-12);
    }
}
