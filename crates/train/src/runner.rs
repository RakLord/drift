//! Evaluating genomes: every genome drives every one of the generation's
//! tracks, in parallel. Seeds depend only on (run seed, generation, track),
//! so results don't depend on thread scheduling.

use std::sync::Arc;

use agent::NnDriver;
use evo::Island;
use nn::{Network, NetworkSpec};
use rayon::prelude::*;
use sim::car::CarParams;
use sim::episode::{Episode, EpisodeConfig, EpisodeResult};
use sim::scoring::ScoringParams;
use sim::track::Track;
use sim::track::generator::{self, GeneratorParams};

/// Everything a run needs besides the network.
#[derive(Clone)]
pub struct Env {
    pub car: CarParams,
    pub scoring: ScoringParams,
    pub episode: EpisodeConfig,
}

/// splitmix64: spreads related numbers into unrelated seeds.
pub fn mix(a: u64, b: u64) -> u64 {
    let mut z = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Fresh generated tracks for a generation (skipping seeds that fail).
pub fn generation_tracks(params: &GeneratorParams, run_seed: u64, generation: u32, count: u32) -> Vec<Arc<Track>> {
    let base = mix(run_seed, u64::from(generation));
    let mut tracks = vec![];
    let mut candidate = 0u64;
    while tracks.len() < count as usize {
        let seed = mix(base, candidate) % 1_000_000_000;
        candidate += 1;
        if let Ok(def) = generator::generate(params, seed)
            && let Ok(track) = Track::build(def)
        {
            tracks.push(Arc::new(track));
        }
        assert!(candidate < 1000, "the track generator keeps failing; check config/generator.toml");
    }
    tracks
}

/// Episode seeds for a generation: one per track, shared by every genome so
/// they all face the same noise and decision timing.
pub fn episode_seeds(run_seed: u64, generation: u32, tracks: usize) -> Vec<u64> {
    (0..tracks).map(|k| mix(mix(run_seed, u64::from(generation)), 1000 + k as u64)).collect()
}

/// Drive one network round every track.
pub fn run_genome(spec: &NetworkSpec, params: &[f32], tracks: &[Arc<Track>], seeds: &[u64], env: &Env) -> Vec<EpisodeResult> {
    let mut driver = NnDriver::new(Network::new(spec.clone(), params.to_vec()));
    tracks
        .iter()
        .zip(seeds)
        .map(|(track, &seed)| {
            let episode = Episode::new(track.clone(), env.car.clone(), env.scoring.clone(), env.episode.clone(), seed);
            episode.run(&mut driver)
        })
        .collect()
}

/// Results for every genome: `[island][genome][track]`.
pub fn evaluate(islands: &[Island], tracks: &[Arc<Track>], seeds: &[u64], env: &Env) -> Vec<Vec<Vec<EpisodeResult>>> {
    let jobs: Vec<(usize, usize)> =
        islands.iter().enumerate().flat_map(|(i, island)| (0..island.genomes.len()).map(move |g| (i, g))).collect();
    let flat: Vec<Vec<EpisodeResult>> = jobs
        .par_iter()
        .map(|&(i, g)| run_genome(&islands[i].spec, &islands[i].genomes[g].params, tracks, seeds, env))
        .collect();
    let mut flat = flat.into_iter();
    islands.iter().map(|island| flat.by_ref().take(island.genomes.len()).collect()).collect()
}
