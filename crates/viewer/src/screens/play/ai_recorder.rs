//! Records each AI run in the background so `X` can save the last one as a
//! replay. Only whole AI runs count: taking over (Tab) drops the recording.

use std::path::PathBuf;

use agent::replay_file::{self, Replay, ReplayHeader};
use sim::World;
use sim::episode::{EndReason, EpisodeResult, EpisodeStats, TICK};
use sim::recording::Recorder;
use sim::scoring::Scorer;

use crate::input::ai::AiDriver;

pub struct AiRecorder {
    recorder: Option<Recorder>,
    /// `AiDriver::decisions` when the last decision was recorded.
    seen: u64,
    last_run: Option<Replay>,
}

impl AiRecorder {
    pub fn new() -> Self {
        Self { recorder: None, seen: 0, last_run: None }
    }

    /// A run is starting: record it if the AI is driving.
    pub fn start(&mut self, ai: Option<&AiDriver>, world: &World, scorer: &Scorer) {
        self.recorder = ai.filter(|ai| ai.enabled).map(|ai| {
            self.seen = ai.decisions;
            Recorder::new(world, scorer)
        });
    }

    /// Before a tick: note a fresh decision, or stop if you took over.
    pub fn before_tick(&mut self, ai: Option<&AiDriver>) {
        let Some(ai) = ai.filter(|ai| ai.enabled) else {
            self.recorder = None;
            return;
        };
        if let Some(recorder) = &mut self.recorder
            && ai.decisions != self.seen
            && let Some(observation) = &ai.last_observation
        {
            recorder.on_decision(observation);
            self.seen = ai.decisions;
        }
    }

    pub fn after_tick(&mut self, world: &World, scorer: &Scorer) {
        if let Some(recorder) = &mut self.recorder {
            recorder.on_tick(world, scorer);
        }
    }

    /// The run ended: keep it as the one `X` saves.
    pub fn finish(&mut self, ai: &AiDriver, world: &World, scorer: &Scorer, end: EndReason) {
        let Some(recorder) = self.recorder.take() else { return };
        let Some(genome) = ai.genome() else { return };
        let recording = recorder.finish(world, scorer);
        let result = EpisodeResult {
            score: scorer.total,
            progress: world.progress,
            track_length: world.track.length,
            time: recording.ticks as f64 * TICK,
            end,
            stats: EpisodeStats::default(),
        };
        let h = &genome.header;
        let run = ai.path.parent().and_then(|d| d.file_name()).map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let title = format!("{run} gen {} on {} (saved in the viewer)", h.generation, world.track.def.name);
        let mut header = ReplayHeader::new(title, result, world.track.def.clone(), world.car.params.clone(), scorer.params.clone());
        (header.run, header.generation, header.island, header.fitness) = (run, h.generation, h.island, h.fitness);
        self.last_run = Some(Replay { header, recording, genome: Some(genome), dropped: None });
    }

    /// Save the last finished AI run next to its genome, in replays/.
    pub fn save(&self, ai: Option<&AiDriver>) -> Result<PathBuf, String> {
        let replay = self.last_run.as_ref().ok_or("no finished AI run to save yet")?;
        let ai = ai.ok_or("no AI driving")?;
        let dir = ai.path.parent().map_or_else(|| PathBuf::from("replays"), |d| d.join("replays"));
        let stem = format!("viewer_g{:05}", replay.header.generation);
        let path = (1..).map(|n| dir.join(format!("{stem}_{n}.replay"))).find(|p| !p.exists()).expect("a free name");
        replay_file::save(&path, replay).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agent::genome_file::{self, GenomeHeader, SavedGenome};
    use nn::{Activation, LayerSpec};
    use sim::car::CarParams;
    use sim::scoring::ScoringParams;
    use sim::track::Track;
    use sim::track::generator::{self, GeneratorParams};

    use super::*;

    #[test]
    fn records_an_ai_run_and_saves_it_next_to_the_genome() {
        let dir = std::env::temp_dir().join(format!("drift-airec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("best.genome");
        let spec = agent::driver_spec(vec![LayerSpec { size: 4, activation: Activation::Tanh }]);
        let params = spec.random_params(&mut common::Rng::new(1));
        genome_file::save(&path, &SavedGenome { header: GenomeHeader::new(spec, 7, 0, 1.0, "test"), params }).unwrap();
        let mut ai = AiDriver::new(path, false).unwrap();

        let track = Track::build(generator::generate(&GeneratorParams::default(), 3).unwrap()).unwrap();
        let mut scorer = Scorer::new(ScoringParams::default(), &track);
        let mut world = World::new(Arc::new(track), CarParams::default());
        let mut rec = AiRecorder::new();
        assert!(rec.save(Some(&ai)).is_err(), "nothing to save before a run ends");

        // The play screen's loop, for two seconds.
        rec.start(Some(&ai), &world, &scorer);
        for _ in 0..240 {
            let controls = ai.controls(&world, &scorer);
            rec.before_tick(Some(&ai));
            world.step(controls, TICK);
            scorer.update(&world, world.contacts(), TICK);
            rec.after_tick(&world, &scorer);
        }
        rec.finish(&ai, &world, &scorer, EndReason::TimeUp);

        let saved = rec.save(Some(&ai)).unwrap();
        assert!(saved.starts_with(dir.join("replays")));
        let replay = replay_file::load(&saved).unwrap();
        assert_eq!(replay.recording.ticks, 240);
        assert_eq!(replay.recording.decisions.len(), 60, "one decision every 4 ticks");
        assert_eq!(replay.header.generation, 7);
        assert!(replay.genome.is_some());
        // A second save doesn't overwrite the first.
        assert_ne!(rec.save(Some(&ai)).unwrap(), saved);

        // Taking over mid-run drops the recording.
        rec.start(Some(&ai), &world, &scorer);
        ai.enabled = false;
        rec.before_tick(Some(&ai));
        assert!(rec.recorder.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
