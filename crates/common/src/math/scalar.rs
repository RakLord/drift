pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Where `value` sits between `a` and `b` (0 at `a`, 1 at `b`). Not clamped.
pub fn inverse_lerp(a: f64, b: f64, value: f64) -> f64 {
    if (b - a).abs() < 1e-12 { 0.0 } else { (value - a) / (b - a) }
}

/// Smooth 0..1 ramp between `edge0` and `edge1`, clamped outside.
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = inverse_lerp(edge0, edge1, x).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Move `current` towards `target` by at most `max_delta`. Used for rate limits
/// such as the steering rack.
pub fn approach(current: f64, target: f64, max_delta: f64) -> f64 {
    let delta = target - current;
    if delta.abs() <= max_delta {
        target
    } else {
        current + max_delta.copysign(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_is_rate_limited() {
        assert_eq!(approach(0.0, 1.0, 0.25), 0.25);
        assert_eq!(approach(0.0, -1.0, 0.25), -0.25);
        assert_eq!(approach(0.9, 1.0, 0.25), 1.0);
    }

    #[test]
    fn smoothstep_clamps() {
        assert_eq!(smoothstep(1.0, 2.0, 0.0), 0.0);
        assert_eq!(smoothstep(1.0, 2.0, 3.0), 1.0);
        assert_eq!(smoothstep(1.0, 2.0, 1.5), 0.5);
    }
}
