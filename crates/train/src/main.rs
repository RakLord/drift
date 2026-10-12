//! Headless trainer: evolves driver networks.
//!
//! `cargo run --release -p train -- --name <run> [--generations N]
//! [--config path] [--stage N] [--seed N] [--resume]`
//!
//! Writes everything to `runs/<name>/`: metrics.csv, best.genome (the best
//! driver so far each generation), checkpoints/ (whole population every few
//! generations), and config/ (a snapshot of every setting used).

mod checkpoint;
mod config;
mod metrics;
mod runner;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use agent::genome_file::{self, GenomeHeader, SavedGenome};
use common::Rng;
use common::paths::workspace_path;
use evo::{Curriculum, Ids, Island, migrate};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sim::car::CarParams;
use sim::scoring::ScoringParams;
use sim::track::generator::GeneratorParams;
use sim::track::{Track, TrackDef};

use checkpoint::Population;
use config::TrainingConfig;
use metrics::{MetricsCsv, Report};
use runner::{Env, evaluate, run_genome};

struct Args {
    name: String,
    generations: Option<u32>,
    config: Option<PathBuf>,
    stage: Option<usize>,
    seed: Option<u64>,
    resume: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { name: String::new(), generations: None, config: None, stage: None, seed: None, resume: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--name" => args.name = value("--name")?,
            "--generations" => args.generations = Some(value("--generations")?.parse().map_err(|_| "bad --generations")?),
            "--config" => args.config = Some(PathBuf::from(value("--config")?)),
            "--stage" => args.stage = Some(value("--stage")?.parse().map_err(|_| "bad --stage")?),
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|_| "bad --seed")?),
            "--resume" => args.resume = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if args.name.is_empty() {
        return Err("usage: train --name <run> [--generations N] [--config path] [--stage N] [--seed N] [--resume]".into());
    }
    Ok(args)
}

/// Load a workspace config, falling back to the built-in default.
fn load_or_default<T: DeserializeOwned + Default>(relative: &str) -> T {
    common::config::load(workspace_path(relative)).unwrap_or_else(|err| {
        eprintln!("using built-in defaults for {relative}: {err}");
        T::default()
    })
}

fn snapshot(dir: &Path, name: &str, value: &impl Serialize) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(name), toml::to_string_pretty(value).map_err(std::io::Error::other)?)
}

/// The held-out test set: library tracks (tracks/*.toml) not marked
/// `test = false`.
fn library_tracks() -> Vec<Arc<Track>> {
    let Ok(entries) = std::fs::read_dir(workspace_path("tracks")) else { return vec![] };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| TrackDef::load(p).ok().filter(|def| def.test).and_then(|def| Track::build(def).ok()))
        .map(Arc::new)
        .collect()
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let run_dir = workspace_path("runs").join(&args.name);
    if run_dir.exists() && !args.resume {
        return Err(format!("{} already exists: use --resume, or pick another --name", run_dir.display()));
    }

    let config_path = args.config.clone().unwrap_or_else(|| workspace_path("config/training.toml"));
    let mut cfg = if args.resume && run_dir.join("config/training.toml").exists() {
        TrainingConfig::load(run_dir.join("config/training.toml")).map_err(|e| e.to_string())?
    } else {
        TrainingConfig::load(&config_path).map_err(|e| e.to_string())?
    };
    if let Some(seed) = args.seed {
        cfg.seed = seed;
    }
    if let Some(generations) = args.generations {
        cfg.generations = generations;
    }
    let (car, scoring, generator): (CarParams, ScoringParams, GeneratorParams) = if args.resume {
        let saved = |name: &str| run_dir.join("config").join(name);
        (
            common::config::load(saved("car.toml")).map_err(|e| e.to_string())?,
            common::config::load(saved("scoring.toml")).map_err(|e| e.to_string())?,
            common::config::load(saved("generator.toml")).map_err(|e| e.to_string())?,
        )
    } else {
        (load_or_default("config/car.toml"), load_or_default("config/scoring.toml"), load_or_default("config/generator.toml"))
    };
    let snapshot_dir = run_dir.join("config");
    for result in [
        snapshot(&snapshot_dir, "training.toml", &cfg),
        snapshot(&snapshot_dir, "car.toml", &car),
        snapshot(&snapshot_dir, "scoring.toml", &scoring),
        snapshot(&snapshot_dir, "generator.toml", &generator),
    ] {
        result.map_err(|e| format!("writing config snapshot: {e}"))?;
    }

    let env = Env { car, scoring, episode: cfg.episode.clone() };
    let test_tracks = library_tracks();

    // Start fresh or carry on from the latest checkpoint.
    let mut pop = match checkpoint::latest(&run_dir).filter(|_| args.resume) {
        Some(path) => {
            println!("resuming from {}", path.display());
            checkpoint::load(&path).map_err(|e| e.to_string())?
        }
        None => {
            let mut ids = Ids::default();
            let islands = cfg
                .islands
                .iter()
                .enumerate()
                .map(|(i, island)| {
                    let mut rng = Rng::with_stream(cfg.seed, 0x15_1a_d0 + i as u64);
                    let mut spec = agent::driver_spec(island.hidden.clone());
                    spec.output_bias = cfg.network.output_bias.clone();
                    spec.output_init_scale = cfg.network.output_init_scale;
                    Island::new(i, spec, &cfg.ga, &mut rng, &mut ids)
                })
                .collect();
            Population { generation: 0, stage: 0, ids, islands }
        }
    };
    let mut curriculum = Curriculum::new(cfg.curriculum.clone(), args.stage.unwrap_or(pop.stage));
    let mut csv = MetricsCsv::open(&run_dir.join("metrics.csv"), pop.islands.len()).map_err(|e| e.to_string())?;
    const HEADER_EVERY: u32 = 20;
    let mut first_generation = true;

    let shapes: Vec<String> = pop.islands.iter().map(|i| i.spec.describe()).collect();
    println!(
        "run '{}': {} islands x {} [{}], {} tracks/gen, test tracks: {} ({}), {} threads",
        args.name,
        pop.islands.len(),
        cfg.ga.population,
        shapes.join(", "),
        cfg.tracks.per_generation,
        test_tracks.len(),
        test_tracks.iter().map(|t| t.def.name.as_str()).collect::<Vec<_>>().join(", "),
        rayon::current_num_threads()
    );

    while pop.generation < cfg.generations {
        let generation = pop.generation;
        let started = Instant::now();
        let tracks = runner::generation_tracks(&generator, cfg.seed, generation, cfg.tracks.per_generation);
        let seeds = runner::episode_seeds(cfg.seed, generation, tracks.len());
        // Shaping stages run on softer rules; the final stage on the real ones.
        let stage = curriculum.stage().clone();
        let (scoring, episode) = stage.rules(&env.scoring, &env.episode);
        let stage_env = Env { car: env.car.clone(), scoring, episode };
        let results = evaluate(&pop.islands, &tracks, &seeds, &stage_env);

        for (island, island_results) in pop.islands.iter_mut().zip(&results) {
            island.fitness = island_results.iter().map(|runs| stage.fitness(runs)).collect();
            island.objectives = island_results.iter().map(|runs| stage.objectives(runs)).collect();
        }
        let best = pop
            .islands
            .iter()
            .enumerate()
            .filter_map(|(i, island)| island.best().map(|(g, f)| (i, g, f)))
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .ok_or("no genomes")?;
        let (bi, bg, best_fitness) = best;
        let best_island = &pop.islands[bi];
        let best_genome = &best_island.genomes[bg];

        let mut report = Report::new(generation, &stage.name, &pop.islands, &results, (bi, bg), 0.0);
        let due_test = cfg.tracks.test_every > 0 && generation % cfg.tracks.test_every == 0 && !test_tracks.is_empty();
        if due_test {
            let seeds = runner::episode_seeds(cfg.seed, u32::MAX, test_tracks.len());
            let test_env = Env {
                episode: sim::episode::EpisodeConfig { sensor_noise: 0.0, ..stage_env.episode.clone() },
                ..stage_env.clone()
            };
            let runs = run_genome(&best_island.spec, &best_genome.params, &test_tracks, &seeds, &test_env);
            report.test_fitness = Some(stage.fitness(&runs));
            report.test_laps = Some(runs.iter().map(|r| r.laps()).sum::<f64>() / runs.len() as f64);
        }

        // This generation's best driver is always on disk.
        let note = format!("run {} gen {generation} stage {}", args.name, stage.name);
        let saved = SavedGenome {
            header: GenomeHeader::new(best_island.spec.clone(), generation, bi as u32, best_fitness, note),
            params: best_genome.params.clone(),
        };
        genome_file::save(run_dir.join("best.genome"), &saved).map_err(|e| e.to_string())?;

        let advanced = curriculum.observe(&results[bi][bg]);
        if advanced {
            println!("== curriculum: moving on to stage '{}'", curriculum.stage().name);
        }
        let migration_due = cfg.ga.migration_every > 0 && generation > 0 && generation % cfg.ga.migration_every == 0;
        if migration_due && let Some((from, to)) = migrate(&mut pop.islands, &cfg.ga, &mut pop.ids) {
            println!("== island {to} replaced by a copy of island {from} ({})", pop.islands[to].spec.describe());
        }

        for island in &mut pop.islands {
            let mut rng = Rng::with_stream(runner::mix(cfg.seed, u64::from(generation)), island.index as u64);
            let best = island.best().map(|(g, _)| island.genomes[g].params.clone());
            island.breed(&cfg.ga, &mut rng, &mut pop.ids, generation + 1);
            // New stage: fresh step sizes and a burst of variety around the
            // island's best, so it can explore the new reward.
            if advanced && let Some(best) = best {
                island.restage(&best, &cfg.ga, &mut rng, &mut pop.ids, generation + 1);
            }
        }
        if advanced {
            println!("== sigma reset to {} and {:.0}% of each island reseeded", cfg.ga.sigma_start, cfg.ga.reseed_fraction * 100.0);
        }
        pop.generation += 1;
        pop.stage = curriculum.current;

        report.seconds = started.elapsed().as_secs_f64();
        let ticks: u64 = results.iter().flatten().flatten().map(|r| r.stats.ticks).sum();
        report.steps_per_sec = ticks as f64 / report.seconds.max(1e-9);
        if first_generation || generation % HEADER_EVERY == 0 {
            if first_generation {
                println!("{}", Report::legend());
            }
            println!("{}", Report::header());
        }
        first_generation = false;
        println!("{}", report.console_line());
        csv.write(&report).map_err(|e| e.to_string())?;

        if cfg.checkpoints.every > 0 && pop.generation % cfg.checkpoints.every == 0 {
            checkpoint::save(&checkpoint::path(&run_dir, pop.generation), &pop).map_err(|e| e.to_string())?;
        }
    }
    println!("done: best driver in {}", run_dir.join("best.genome").display());
    Ok(())
}
