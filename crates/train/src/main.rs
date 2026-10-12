//! Headless trainer: evolves driver networks.
//!
//! `cargo run --release -p train -- --name <run> [--generations N]
//! [--config path] [--stage N] [--seed N] [[--track path]... | --no-fixed]
//! [[--pool path]... | --no-pool] [--pooled N] [--generated N] [--resume]`
//!
//! `--track` (repeatable) / `--no-fixed`, `--pool` (repeatable) / `--no-pool`,
//! `--pooled` and `--generated` override `tracks.fixed`, `tracks.pool`,
//! `tracks.pool_per_generation` and `tracks.per_generation`.
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
use agent::replay_file::{self, Replay, ReplayHeader};
use agent::top_file::{self, TopEntry, TopGenomes, TopHeader};
use common::Rng;
use common::paths::workspace_path;
use evo::{Curriculum, Ids, Island, Stage, migrate};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sim::car::CarParams;
use sim::episode::EpisodeResult;
use sim::scoring::ScoringParams;
use sim::track::generator::GeneratorParams;
use sim::track::{Track, TrackDef};

use checkpoint::Population;
use config::{TrainingConfig, WatchConfig};
use metrics::{MetricsCsv, Report, Saved};
use runner::{Env, evaluate, evaluate_some, run_genome};

/// Seed stream for the `[best]` check tracks, apart from the training ones.
const CHECK_STREAM: u64 = 0xC4EC_0B57;

struct Args {
    name: String,
    generations: Option<u32>,
    config: Option<PathBuf>,
    stage: Option<usize>,
    seed: Option<u64>,
    tracks: Option<Vec<String>>,
    pool: Option<Vec<String>>,
    pooled: Option<u32>,
    generated: Option<u32>,
    resume: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        name: String::new(),
        generations: None,
        config: None,
        stage: None,
        seed: None,
        tracks: None,
        pool: None,
        pooled: None,
        generated: None,
        resume: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--name" => args.name = value("--name")?,
            "--generations" => args.generations = Some(value("--generations")?.parse().map_err(|_| "bad --generations")?),
            "--config" => args.config = Some(PathBuf::from(value("--config")?)),
            "--stage" => args.stage = Some(value("--stage")?.parse().map_err(|_| "bad --stage")?),
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|_| "bad --seed")?),
            "--track" => args.tracks.get_or_insert_default().push(value("--track")?),
            "--no-fixed" => args.tracks = Some(vec![]),
            "--pool" => args.pool.get_or_insert_default().push(value("--pool")?),
            "--no-pool" => args.pool = Some(vec![]),
            "--pooled" => args.pooled = Some(value("--pooled")?.parse().map_err(|_| "bad --pooled")?),
            "--generated" => args.generated = Some(value("--generated")?.parse().map_err(|_| "bad --generated")?),
            "--resume" => args.resume = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if args.name.is_empty() {
        return Err("usage: train --name <run> [--generations N] [--config path] [--stage N] [--seed N] [[--track path]... | --no-fixed] [[--pool path]... | --no-pool] [--pooled N] [--generated N] [--resume]".into());
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

/// Load a saved track (workspace-relative path).
fn load_track(relative: &str) -> Result<Arc<Track>, String> {
    let def = TrackDef::load(workspace_path(relative)).map_err(|e| format!("{relative}: {e}"))?;
    Track::build(def).map(Arc::new).map_err(|e| format!("{relative}: {e}"))
}

/// The held-out test set: library tracks (tracks/*.toml) not marked
/// `test = false` and not trained on (fixed or pool).
fn library_tracks<'a>(trained_on: impl Iterator<Item = &'a String>) -> Vec<Arc<Track>> {
    let Ok(entries) = std::fs::read_dir(workspace_path("tracks")) else { return vec![] };
    let trained_on: Vec<PathBuf> = trained_on.filter_map(|f| workspace_path(f).canonicalize().ok()).collect();
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    paths.retain(|p| p.canonicalize().map_or(true, |p| !trained_on.contains(&p)));
    paths.sort();
    paths
        .iter()
        .filter_map(|p| TrackDef::load(p).ok().filter(|def| def.test).and_then(|def| Track::build(def).ok()))
        .map(Arc::new)
        .collect()
}

/// Record the saved best on each of the generation's tracks. Returns how
/// many replays were written.
#[allow(clippy::too_many_arguments)]
fn save_replays(
    run_dir: &Path,
    run: &str,
    genome: &SavedGenome,
    saved: &Saved,
    stage: &str,
    tracks: &[Arc<Track>],
    seeds: &[u64],
    env: &Env,
) -> Result<usize, String> {
    let h = &genome.header;
    let runs = runner::record_genome(&h.spec, &genome.params, tracks, seeds, env);
    for (k, ((result, recording), track)) in runs.into_iter().zip(tracks).enumerate() {
        // Deterministic: the recording is the run that was scored.
        if saved.runs.get(k) != Some(&result) {
            eprintln!("warning: replay of gen {} track {k} differs from its scored run", h.generation);
        }
        let title = format!("{run} gen {} ({stage}) on {}", h.generation, track.def.name);
        let mut header = ReplayHeader::new(title, result, track.def.clone(), env.car.clone(), env.scoring.clone());
        (header.run, header.generation, header.island, header.stage, header.fitness) =
            (run.to_owned(), h.generation, h.island, stage.to_owned(), h.fitness);
        let replay = Replay { header, recording, genome: Some(genome.clone()), dropped: None };
        let path = run_dir.join("replays").join(format!("g{:05}_t{k}.replay", h.generation));
        replay_file::save(&path, &replay).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(tracks.len())
}

/// Write top.genomes for the live viewer: the top few of each island by
/// fitness, then the best drifters not already in.
#[allow(clippy::too_many_arguments)]
fn save_top(
    path: &Path,
    run: &str,
    generation: u32,
    stage: &str,
    islands: &[Island],
    results: &[Vec<Vec<EpisodeResult>>],
    watch: &WatchConfig,
    env: &Env,
) -> Result<(), String> {
    let mut ranked: Vec<(usize, usize)> =
        islands.iter().enumerate().flat_map(|(i, island)| (0..island.fitness.len()).map(move |g| (i, g))).collect();
    ranked.sort_by(|&(ai, ag), &(bi, bg)| islands[bi].fitness[bg].total_cmp(&islands[ai].fitness[ag]));
    let quality = |&(i, g): &(usize, usize)| {
        let runs = &results[i][g];
        runs.iter().map(evo::fitness::drift_quality).sum::<f64>() / runs.len().max(1) as f64
    };

    let mut picked: Vec<((usize, usize), bool)> = vec![];
    for island in 0..islands.len() {
        picked.extend(ranked.iter().filter(|(i, _)| *i == island).take(watch.per_island).map(|&k| (k, false)));
    }
    let mut by_quality = ranked.clone();
    by_quality.sort_by(|a, b| quality(b).total_cmp(&quality(a)));
    let drifters: Vec<_> =
        by_quality.into_iter().filter(|k| !picked.iter().any(|(p, _)| p == k)).take(watch.drifters).collect();
    picked.extend(drifters.into_iter().map(|k| (k, true)));

    let mut entries = vec![];
    let mut genomes = vec![];
    for ((i, g), drifter) in picked {
        let island = &islands[i];
        let fitness = island.fitness[g];
        let rank = ranked.iter().position(|&k| k == (i, g)).unwrap_or(0) as u32 + 1;
        entries.push(TopEntry { id: island.genomes[g].id, island: i as u32, rank, fitness, drift_quality: quality(&(i, g)), drifter });
        let header = GenomeHeader::new(island.spec.clone(), generation, i as u32, fitness, format!("{run} gen {generation} #{rank}"));
        genomes.push(SavedGenome { header, params: island.genomes[g].params.clone() });
    }
    let header = TopHeader {
        run: run.to_owned(),
        generation,
        stage: stage.to_owned(),
        episode: env.episode.clone(),
        scoring: env.scoring.clone(),
        car: env.car.clone(),
        entries,
    };
    top_file::save(path, &TopGenomes { header, genomes }).map_err(|e| format!("{}: {e}", path.display()))
}

/// Pick the driver to save as best.genome. Without `[best]`, the fittest.
/// With it, the top few by fitness also drive some extra tracks (`spare`:
/// pool tracks not driven this generation, then generated ones), and the
/// best over all its runs wins.
#[allow(clippy::too_many_arguments)]
fn pick_saved(
    islands: &[Island],
    results: &[Vec<Vec<EpisodeResult>>],
    cfg: &TrainingConfig,
    generator: &GeneratorParams,
    spare: &[Arc<Track>],
    generation: u32,
    stage: &Stage,
    env: &Env,
) -> Result<Saved, String> {
    let mut ranked: Vec<(usize, usize)> =
        islands.iter().enumerate().flat_map(|(i, island)| (0..island.fitness.len()).map(move |g| (i, g))).collect();
    ranked.sort_by(|&(ai, ag), &(bi, bg)| islands[bi].fitness[bg].total_cmp(&islands[ai].fitness[ag]));
    let &(island, genome) = ranked.first().ok_or("no genomes")?;
    if !cfg.best.enabled() {
        let runs = results[island][genome].clone();
        return Ok(Saved { island, genome, fitness: stage.fitness(&runs), runs, rank: 0 });
    }

    let chosen = &ranked[..cfg.best.candidates.min(ranked.len())];
    let check_seed = runner::mix(cfg.seed, CHECK_STREAM);
    let mut tracks: Vec<Arc<Track>> = spare.iter().take(cfg.best.tracks as usize).cloned().collect();
    let generated = cfg.best.tracks - tracks.len() as u32;
    tracks.extend(runner::generation_tracks(generator, check_seed, generation, generated));
    let seeds = runner::episode_seeds(check_seed, generation, tracks.len());
    let checks = evaluate_some(islands, chosen, &tracks, &seeds, env);
    chosen
        .iter()
        .zip(checks)
        .enumerate()
        .map(|(rank, (&(island, genome), check))| {
            let mut runs = results[island][genome].clone();
            runs.extend(check);
            Saved { island, genome, fitness: stage.fitness(&runs), runs, rank }
        })
        // On a tie the higher-ranked one wins.
        .reduce(|best, next| if next.fitness > best.fitness { next } else { best })
        .ok_or_else(|| "no genomes".to_owned())
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
    // A run exists once it has written its config (the launcher may have
    // already created the folder for train.log).
    if run_dir.join("config").exists() && !args.resume {
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
    if let Some(tracks) = &args.tracks {
        cfg.tracks.fixed = tracks.clone();
    }
    if let Some(pool) = &args.pool {
        cfg.tracks.pool = pool.clone();
    }
    if let Some(pooled) = args.pooled {
        cfg.tracks.pool_per_generation = pooled;
    }
    if let Some(generated) = args.generated {
        cfg.tracks.per_generation = generated;
    }
    if let Some(both) = cfg.tracks.pool.iter().find(|p| cfg.tracks.fixed.contains(p)) {
        return Err(format!("{both} is in both tracks.fixed and tracks.pool: pick one"));
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
    let fixed_tracks = cfg.tracks.fixed.iter().map(|f| load_track(f)).collect::<Result<Vec<_>, _>>()?;
    let pool_tracks = cfg.tracks.pool.iter().map(|f| load_track(f)).collect::<Result<Vec<_>, _>>()?;
    if fixed_tracks.is_empty() && cfg.tracks.pool_draw() == 0 && cfg.tracks.per_generation == 0 {
        return Err("no training tracks: set tracks.per_generation, tracks.fixed or tracks.pool + pool_per_generation".into());
    }
    if !pool_tracks.is_empty() && cfg.tracks.pool_draw() == 0 {
        eprintln!("warning: tracks.pool has {} tracks but pool_per_generation is 0, so only the [best] check runs use it", pool_tracks.len());
    }
    let test_tracks = library_tracks(cfg.tracks.trained_on());

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
    // Jumping straight into a polish stage (--stage): fine steps from the start.
    if curriculum.current != pop.stage
        && let Some(polish) = &curriculum.stage().polish
    {
        for g in pop.islands.iter_mut().flat_map(|i| i.genomes.iter_mut()) {
            g.sigma = polish.sigma_start;
        }
        println!("== polishing: sigma set to {}", polish.sigma_start);
    }
    let mut csv = MetricsCsv::open(&run_dir.join("metrics.csv"), pop.islands.len()).map_err(|e| e.to_string())?;
    const HEADER_EVERY: u32 = 20;
    let mut first_generation = true;

    let shapes: Vec<String> = pop.islands.iter().map(|i| i.spec.describe()).collect();
    println!(
        "run '{}': {} islands x {} [{}], {} generated tracks/gen, fixed tracks: [{}], pool: {} of [{}] per gen, test tracks: {} ({}), best.genome check: {}, {} threads",
        args.name,
        pop.islands.len(),
        cfg.ga.population,
        shapes.join(", "),
        curriculum.stage().tracks_per_generation(cfg.tracks.per_generation),
        fixed_tracks.iter().map(|t| t.def.name.as_str()).collect::<Vec<_>>().join(", "),
        cfg.tracks.pool_draw(),
        pool_tracks.iter().map(|t| t.def.name.as_str()).collect::<Vec<_>>().join(", "),
        test_tracks.len(),
        test_tracks.iter().map(|t| t.def.name.as_str()).collect::<Vec<_>>().join(", "),
        if cfg.best.enabled() {
            format!("top {} on {} extra tracks", cfg.best.candidates, cfg.best.tracks)
        } else {
            "off".to_owned()
        },
        rayon::current_num_threads()
    );

    while pop.generation < cfg.generations {
        let generation = pop.generation;
        let started = Instant::now();
        let stage = curriculum.stage().clone();
        // A polish stage has its own (finer) GA settings and track count.
        let ga = stage.ga(&cfg.ga);
        let pool_order: Vec<Arc<Track>> =
            runner::pool_order(cfg.seed, generation, pool_tracks.len()).into_iter().map(|k| pool_tracks[k].clone()).collect();
        let (pooled, spare) = pool_order.split_at(cfg.tracks.pool_draw());
        let mut tracks = fixed_tracks.clone();
        tracks.extend_from_slice(pooled);
        tracks.extend(runner::generation_tracks(&generator, cfg.seed, generation, stage.tracks_per_generation(cfg.tracks.per_generation)));
        let seeds = runner::episode_seeds(cfg.seed, generation, tracks.len());
        // Shaping stages run on softer rules; the final stage on the real ones.
        let (scoring, episode) = stage.rules(&env.scoring, &env.episode);
        let stage_env = Env { car: env.car.clone(), scoring, episode };
        let results = evaluate(&pop.islands, &tracks, &seeds, &stage_env);

        for (island, island_results) in pop.islands.iter_mut().zip(&results) {
            island.fitness = island_results.iter().map(|runs| stage.fitness(runs)).collect();
            island.objectives = island_results.iter().map(|runs| stage.objectives(runs)).collect();
        }
        let saved = pick_saved(&pop.islands, &results, &cfg, &generator, spare, generation, &stage, &stage_env)?;
        let best_island = &pop.islands[saved.island];
        let best_genome = &best_island.genomes[saved.genome];

        let mut report = Report::new(generation, &stage.name, &pop.islands, &results, &saved, 0.0);
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
        let file = SavedGenome {
            header: GenomeHeader::new(best_island.spec.clone(), generation, saved.island as u32, saved.fitness, note),
            params: best_genome.params.clone(),
        };
        genome_file::save(run_dir.join("best.genome"), &file).map_err(|e| e.to_string())?;
        let top = run_dir.join("top.genomes");
        save_top(&top, &args.name, generation, &stage.name, &pop.islands, &results, &cfg.watch, &stage_env)?;
        if cfg.replays.every > 0 && generation % cfg.replays.every == 0 {
            let written = save_replays(&run_dir, &args.name, &file, &saved, &stage.name, &tracks, &seeds, &stage_env)?;
            report.replays = written;
        }

        // Judged on all the saved driver's runs, so one lucky draw can't
        // move the curriculum on.
        let advanced = curriculum.observe(&saved.runs);
        if advanced {
            let next = curriculum.stage();
            println!("== curriculum: moving on to stage '{}'", next.name);
            if next.polish.is_some() {
                println!("== polishing: {} generated tracks/gen", next.tracks_per_generation(cfg.tracks.per_generation));
            }
        }
        let migration_due = ga.migration_every > 0 && generation > 0 && generation % ga.migration_every == 0;
        if migration_due && let Some((from, to)) = migrate(&mut pop.islands, &ga, &mut pop.ids) {
            println!("== island {to} replaced by a copy of island {from} ({})", pop.islands[to].spec.describe());
        }

        // The next stage's settings, if it just changed.
        let next_ga = curriculum.stage().ga(&cfg.ga);
        for island in &mut pop.islands {
            let mut rng = Rng::with_stream(runner::mix(cfg.seed, u64::from(generation)), island.index as u64);
            let best = island.best().map(|(g, _)| island.genomes[g].params.clone());
            island.breed(&ga, &mut rng, &mut pop.ids, generation + 1);
            // New stage: fresh step sizes and a burst of variety around the
            // island's best, so it can explore the new reward (a polish
            // stage only resets the step sizes).
            if advanced && let Some(best) = best {
                island.restage(&best, &next_ga, &mut rng, &mut pop.ids, generation + 1);
            }
        }
        if advanced {
            println!(
                "== sigma reset to {} and {:.0}% of each island reseeded",
                next_ga.sigma_start,
                next_ga.reseed_fraction * 100.0
            );
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
