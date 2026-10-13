//! A genome driven fresh on any track, for a video of a run the trainer
//! never saved: the trainer's own episode code with the run's settings, so
//! it drives exactly as it did in training. Saved as a replay beside the
//! genome so the video can be made again.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent::NnDriver;
use agent::genome_file;
use agent::replay_file::{self, Replay, ReplayHeader};
use nn::Network;
use sim::episode::Episode;
use sim::track::generator;
use sim::track::{Track, TrackDef};

use crate::input::ai::RunSettings;
use crate::paths::workspace_file;
use crate::screens::play::configs::Configs;

/// The track to drive: a library file, or a generated one by seed (with
/// `config/generator.toml`). Also a short name for file names.
pub fn track(file: Option<&str>, seed: Option<u64>) -> Result<(TrackDef, String), String> {
    match (file, seed) {
        (Some(file), _) => {
            let def = TrackDef::load(workspace_file(file)).map_err(|e| format!("{file}: {e}"))?;
            let name = Path::new(file).file_stem().map_or_else(|| "track".into(), |s| s.to_string_lossy().into_owned());
            Ok((def, name))
        }
        (None, Some(seed)) => {
            let params = Configs::load(None, &mut vec![]).generator();
            let def = generator::generate(&params, seed).map_err(|f| f.lines(3).join("; "))?;
            Ok((def, format!("gen{seed}")))
        }
        (None, None) => Err("a video of a genome needs --track <file> or --seed N".to_owned()),
    }
}

/// Drive `genome` round `track` once, recording it; save the replay as
/// `<genome dir>/replays/video_gNNNNN_<name>.replay` and return its path.
pub fn record(genome: &Path, track: TrackDef, name: &str) -> Result<PathBuf, String> {
    let replay = drive(genome, track)?;
    let dir = genome.parent().unwrap_or(Path::new(".")).join("replays");
    let path = dir.join(format!("video_g{:05}_{name}.replay", replay.header.generation));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    replay_file::save(&path, &replay).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn drive(genome: &Path, def: TrackDef) -> Result<Replay, String> {
    let saved = genome_file::load(genome).map_err(|e| format!("{}: {e}", genome.display()))?;
    let settings = RunSettings::for_genome(genome);
    let configs = Configs::load(None, &mut vec![]);
    let car = settings.car.unwrap_or_else(|| configs.car());
    let scoring = settings.scoring.unwrap_or_else(|| configs.scoring());
    let track = Track::build(def.clone()).map_err(|e| format!("{}: track doesn't build: {e}", def.name))?;

    let mut driver = NnDriver::new(Network::new(saved.header.spec.clone(), saved.params.clone()));
    let episode = Episode::new(Arc::new(track), car.clone(), scoring.clone(), settings.episode, 0);
    let (result, recording) = episode.run_recorded(&mut driver);

    let h = &saved.header;
    let run = crate::screens::run_of(genome).unwrap_or_default();
    let title = format!("{run} gen {} on {}", h.generation, def.name);
    let mut header = ReplayHeader::new(title, result, def, car, scoring);
    (header.run, header.generation, header.island, header.fitness) = (run, h.generation, h.island, h.fitness);
    Ok(Replay { header, recording, genome: Some(saved), dropped: None })
}

#[cfg(test)]
mod tests {
    use agent::genome_file::{GenomeHeader, SavedGenome};
    use nn::{Activation, LayerSpec};
    use sim::car::CarParams;
    use sim::episode::EpisodeConfig;
    use sim::scoring::ScoringParams;
    use sim::track::generator::GeneratorParams;

    use super::*;

    #[test]
    fn records_the_same_run_an_unrecorded_episode_gives() {
        let dir = std::env::temp_dir().join(format!("drift-fresh-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config")).unwrap();
        let path = dir.join("best.genome");
        let spec = agent::driver_spec(vec![LayerSpec { size: 4, activation: Activation::Tanh }]);
        let params = spec.random_params(&mut common::Rng::new(5));
        let header = GenomeHeader::new(spec.clone(), 12, 1, 3.0, "test");
        genome_file::save(&path, &SavedGenome { header, params: params.clone() }).unwrap();
        // Known run settings, so the comparison doesn't depend on config/.
        let episode = EpisodeConfig { max_secs: 5.0, ..RunSettings::for_genome(&path).episode };
        std::fs::write(dir.join("config/training.toml"), toml::to_string(&Training { episode: episode.clone() }).unwrap())
            .unwrap();
        std::fs::write(dir.join("config/car.toml"), toml::to_string(&CarParams::default()).unwrap()).unwrap();
        std::fs::write(dir.join("config/scoring.toml"), toml::to_string(&ScoringParams::default()).unwrap()).unwrap();

        let def = generator::generate(&GeneratorParams::default(), 4).unwrap();
        let saved = record(&path, def.clone(), "gen4").unwrap();
        assert_eq!(saved, dir.join("replays/video_g00012_gen4.replay"));
        let replay = replay_file::load(&saved).unwrap();
        assert_eq!(replay.header.generation, 12);
        assert!(replay.genome.is_some());

        let mut driver = NnDriver::new(Network::new(spec, params));
        let track = Arc::new(Track::build(def).unwrap());
        let plain = Episode::new(track, CarParams::default(), ScoringParams::default(), episode, 0).run(&mut driver);
        assert_eq!(replay.header.result, plain);
        assert_eq!(replay.recording.ticks, plain.stats.ticks);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[derive(serde::Serialize)]
    struct Training {
        episode: EpisodeConfig,
    }
}
