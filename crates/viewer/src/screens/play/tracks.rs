//! Which track the play screen is on: the library track, or a generated one.

use sim::track::generator::{self, GeneratorParams};
use sim::track::{Track, TrackDef};

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
