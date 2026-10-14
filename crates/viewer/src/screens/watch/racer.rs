//! One car in the live multi-car view: a genome driving its own episode,
//! exactly as in training, plus what the view needs (skid marks, the pose
//! before the last tick for smooth drawing, the latest observation).

use std::sync::Arc;

use agent::NnDriver;
use agent::genome_file::SavedGenome;
use agent::top_file::{TopEntry, TopHeader};
use common::Vec2;
use common::math::{angle_diff, wrap_angle};
use nn::Network;
use sim::car::CarState;
use sim::episode::{EndReason, Episode};
use sim::sensors::observation::Observation;
use sim::track::Track;
use sim::{Controller, Controls};

use crate::render::effects::skidmarks::SkidMarks;
use crate::render::effects::slide_intensity;

/// The network, remembering what it last saw (for the network view).
struct Driver {
    nn: NnDriver,
    last: Option<Observation>,
    decisions: u64,
}

impl Controller for Driver {
    fn decide(&mut self, observation: &Observation) -> Controls {
        self.last = Some(observation.clone());
        self.decisions += 1;
        self.nn.decide(observation)
    }

    fn reset(&mut self) {
        self.nn.reset();
        self.last = None;
    }
}

pub struct Racer {
    pub entry: TopEntry,
    /// "island 3  top 5 #12": its island, why it's racing, its fitness rank.
    pub label: String,
    /// The label without the island ("top 5 #12"), for rows under an
    /// island heading.
    pub why: String,
    /// Its network shape, e.g. "111-48t-5".
    pub shape: String,
    episode: Episode,
    driver: Driver,
    pub skids: SkidMarks,
    previous: (Vec2, f64),
    pub end: Option<EndReason>,
}

impl Racer {
    pub fn new(entry: TopEntry, genome: &SavedGenome, header: &TopHeader, track: Arc<Track>, seed: u64) -> Self {
        let network = Network::new(genome.header.spec.clone(), genome.params.clone());
        let mut driver = Driver { nn: NnDriver::new(network, &header.episode), last: None, decisions: 0 };
        driver.reset();
        let episode = Episode::new(track, header.car.clone(), header.scoring.clone(), header.episode.clone(), seed);
        let s = &episode.world.car.state;
        let previous = (s.position, s.heading);
        let why = format!("{} #{}", reason(&entry, header), entry.rank);
        let label = format!("island {}  {why}", entry.island);
        let shape = genome.header.spec.describe();
        Self { entry, label, why, shape, episode, driver, skids: SkidMarks::new(), previous, end: None }
    }

    /// One sim tick (nothing once the run has ended).
    pub fn tick(&mut self) {
        if self.end.is_some() {
            return;
        }
        let s = &self.episode.world.car.state;
        self.previous = (s.position, s.heading);
        self.end = self.episode.step(&mut self.driver);
        let car = &self.episode.world.car;
        for (i, (wheel, position)) in car.state.wheels.iter().zip(car.state.wheel_positions(&car.params)).enumerate() {
            self.skids.track(i, position, slide_intensity(wheel, &car.params));
        }
    }

    pub fn episode(&self) -> &Episode {
        &self.episode
    }

    /// The car between the last two ticks (`alpha` 0..1), for drawing.
    pub fn shown_state(&self, alpha: f64) -> CarState {
        let mut state = self.episode.world.car.state.clone();
        if self.end.is_none() {
            let (position, heading) = self.previous;
            state.position = position.lerp(state.position, alpha);
            state.heading = wrap_angle(heading + angle_diff(heading, state.heading) * alpha);
        }
        state
    }

    /// Banked score plus the combo still running.
    pub fn live_score(&self) -> f64 {
        let scorer = &self.episode.scorer;
        scorer.total.total() + if scorer.combo.active { scorer.combo.value().total() } else { 0.0 }
    }

    pub fn network(&self) -> &Network {
        &self.driver.nn.network
    }

    /// The latest observation and how many decisions have been made.
    pub fn last_decision(&self) -> (Option<&Observation>, u64) {
        (self.driver.last.as_ref(), self.driver.decisions)
    }
}

/// Why it's racing: "drift pick" (picked for drift quality), else "top k"
/// (one of its island's k fittest; k counted from the file, as the
/// trainer's `per_island` isn't written into it).
fn reason(entry: &TopEntry, header: &TopHeader) -> String {
    if entry.drifter {
        return "drift pick".to_owned();
    }
    let k = header.entries.iter().filter(|e| e.island == entry.island && !e.drifter).count();
    format!("top {k}")
}
