use std::f64::consts::{PI, TAU};

/// Wrap an angle into (-PI, PI].
pub fn wrap_angle(angle: f64) -> f64 {
    let wrapped = angle.rem_euclid(TAU);
    if wrapped > PI { wrapped - TAU } else { wrapped }
}

/// Shortest signed rotation from `from` to `to`, in (-PI, PI].
pub fn angle_diff(from: f64, to: f64) -> f64 {
    wrap_angle(to - from)
}

pub fn deg_to_rad(deg: f64) -> f64 {
    deg.to_radians()
}

pub fn rad_to_deg(rad: f64) -> f64 {
    rad.to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn wrap_stays_in_range() {
        for i in -100..=100 {
            let a = wrap_angle(i as f64 * 0.37);
            assert!(a > -PI && a <= PI, "{a}");
        }
        assert!(close(wrap_angle(PI), PI));
        assert!(close(wrap_angle(-PI), PI));
        assert!(close(wrap_angle(3.0 * PI), PI));
    }

    #[test]
    fn diff_takes_short_way_round() {
        assert!(close(angle_diff(deg_to_rad(170.0), deg_to_rad(-170.0)), deg_to_rad(20.0)));
        assert!(close(angle_diff(deg_to_rad(-170.0), deg_to_rad(170.0)), deg_to_rad(-20.0)));
    }
}
