//! Which track the play screen is on: the library track, or a generated one.

use std::path::PathBuf;

use sim::track::generator::{self, GeneratorParams};
use sim::track::{Track, TrackDef};

use crate::paths::workspace_file;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackSource {
    Library,
    Generated(u64),
}

/// Build the track for `source`. On failure, returns lines explaining why.
pub fn build(source: TrackSource, library: TrackDef, generator: &GeneratorParams) -> Result<(Track, TrackDef), Vec<String>> {
    let def = match source {
        TrackSource::Library => library,
        TrackSource::Generated(seed) => generator::generate(generator, seed).map_err(|failure| failure.lines(3))?,
    };
    let track = Track::build(def.clone()).map_err(|e| vec![e.to_string()])?;
    Ok((track, def))
}

/// Save a generated track into the library folder.
pub fn keep(def: &TrackDef, seed: u64) -> Result<PathBuf, String> {
    let path = workspace_file("tracks").join(format!("generated_{seed}.toml"));
    let header = format!("# Generated from seed {seed}, kept from the viewer.\n\n");
    let body = toml::to_string(def).map_err(|e| e.to_string())?;
    std::fs::write(&path, header + &body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
