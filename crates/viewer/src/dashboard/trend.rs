//! Trend lines: a trailing rolling mean over the generations before each
//! point (never after, so the live right edge means the same as the rest).

/// Rows per trend window for `visible` rows on screen: about a twentieth
/// of them, 5 to 100. None when too few to be worth smoothing.
pub fn window_for(visible: usize) -> Option<usize> {
    (visible >= 20).then(|| (visible / 20).clamp(5, 100))
}

/// Mean of the finite values among each row and the `window - 1` rows
/// before it; NaN where there are none (gaps don't drag the mean down).
pub fn rolling_mean(values: &[f64], window: usize) -> Vec<f64> {
    let window = window.max(1);
    let (mut sum, mut count) = (0.0, 0usize);
    let mut out = Vec::with_capacity(values.len());
    for (i, &v) in values.iter().enumerate() {
        if v.is_finite() {
            sum += v;
            count += 1;
        }
        if i >= window
            && let Some(&old) = values.get(i - window).filter(|v| v.is_finite())
        {
            sum -= old;
            count -= 1;
        }
        out.push(if count > 0 { sum / count as f64 } else { f64::NAN });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_the_rows_up_to_each_point() {
        let mean = rolling_mean(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
        assert_eq!(mean, vec![1.0, 1.5, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn skips_gaps_and_is_nan_only_when_the_window_is_empty() {
        let nan = f64::NAN;
        let mean = rolling_mean(&[2.0, nan, 4.0, nan, nan, nan], 2);
        assert_eq!(mean[..4], [2.0, 2.0, 4.0, 4.0]);
        assert!(mean[4].is_nan() && mean[5].is_nan());
    }

    #[test]
    fn window_grows_with_the_view_within_bounds() {
        assert_eq!(window_for(10), None);
        assert_eq!(window_for(20), Some(5));
        assert_eq!(window_for(600), Some(30));
        assert_eq!(window_for(5000), Some(100));
    }
}
