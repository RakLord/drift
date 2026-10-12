//! Every track in the library must build.

use std::path::Path;

use sim::track::{Track, TrackDef, TrackStats};

#[test]
fn library_tracks_build() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tracks");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            let def = TrackDef::load(&path).unwrap_or_else(|e| panic!("{e}"));
            let track = Track::build(def).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let stats = TrackStats::of(&track);
            println!(
                "{}: {:.0} m, {} segments, {} corners ({:.1}/km), sharpest r {:.0} m, {} hairpins, {} crossings, {:.0}% hard",
                path.display(),
                track.length,
                track.segments.len(),
                stats.corners,
                stats.corners_per_km,
                stats.sharpest.unwrap_or(f64::NAN),
                stats.hairpins,
                stats.crossings,
                stats.hard_share * 100.0
            );
            assert_eq!(stats.corners, track.corners.len());
            assert!((0.0..=1.0).contains(&stats.hard_share));
            assert!(stats.hairpins <= stats.corners);
            for c in &track.corners {
                println!(
                    "  {} {:>3.0} deg  r {:>4.0} m  at {:>4.0}-{:>4.0} m  clip {:?}  outer {:.0}%  walls in/out {}/{}",
                    if c.direction > 0.0 { "L" } else { "R" },
                    c.angle.to_degrees(),
                    c.min_radius,
                    c.start,
                    c.end,
                    c.clip.map(|f| (f * 100.0).round()),
                    c.outer_coverage * 100.0,
                    c.inside_hard,
                    c.outside_hard,
                );
            }
            count += 1;
        }
    }
    assert!(count > 0, "no tracks in {}", dir.display());
}

#[test]
fn driver_prep_counts_down_to_the_next_corner() {
    use sim::sensors::driver_prep::{features, upcoming};
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/default_track.toml");
    let track = Track::build(TrackDef::load(path).unwrap()).unwrap();
    let first = track.corners[0];

    // Halfway along the longest straight, the corner after it is next.
    let n = track.corners.len();
    let (next, gap) = (0..n)
        .map(|i| (i, (track.corners[i].start - track.corners[(i + n - 1) % n].end).rem_euclid(track.length)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let before = upcoming(&track, track.corners[next].start - gap / 2.0)[0].unwrap();
    assert!((before.distance_to - gap / 2.0).abs() < 1.0 && before.progress == 0.0, "{before:?}, gap {gap:.0} m");

    let inside = upcoming(&track, first.start + first.length(track.length) / 2.0)[0].unwrap();
    assert_eq!(inside.distance_to, 0.0);
    assert!((inside.progress - 0.5).abs() < 0.05);

    // Just past the last corner's exit, the next corner wraps round the lap.
    let last = track.corners[track.corners.len() - 1];
    let wrapped = upcoming(&track, last.end + 1.0)[0].unwrap();
    assert_eq!(wrapped.direction, first.direction);

    // Inside a corner that wraps past the start line, that corner comes first.
    if last.end < last.start {
        let in_wrap = upcoming(&track, last.end / 2.0)[0].unwrap();
        assert_eq!(in_wrap.distance_to, 0.0);
    }

    let f = features(&upcoming(&track, 0.0));
    assert!(f.iter().all(|v| (-1.0..=1.0).contains(v)), "{f:?}");
}

#[test]
fn skidpad_is_not_a_test_track_but_courses_are() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let load = |path: &str| TrackDef::load(root.join(path)).unwrap();
    assert!(!load("tracks/skidpad.toml").test);
    // The default track has no `test` line.
    assert!(load("config/default_track.toml").test, "tracks are test tracks unless they opt out");
}
