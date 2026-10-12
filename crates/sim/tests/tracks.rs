//! Every track in the library must build.

use std::path::Path;

use sim::track::{Track, TrackDef};

#[test]
fn library_tracks_build() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tracks");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            let def = TrackDef::load(&path).unwrap_or_else(|e| panic!("{e}"));
            let track = Track::build(def).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            println!("{}: {:.0} m, {} segments", path.display(), track.length, track.segments.len());
            count += 1;
        }
    }
    assert!(count > 0, "no tracks in {}", dir.display());
}
