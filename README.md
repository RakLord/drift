# Drift

A top-down 2D drift sim where small neural networks learn to drift, trained with a genetic algorithm (neuroevolution). The car physics, the neural network and the genetic algorithm are all written from scratch in Rust.

Drivers are judged Formula Drift style: **line** (clip points and outer zones), **angle** and **style**, with speed as a multiplier. Points only build up while drifting and are banked on a clean exit.

Design notes, decisions and the current status live in [`PLAN.md`](PLAN.md).

## Requirements

- **Rust 1.85 or newer** (the workspace uses edition 2024). Install it with [rustup](https://rustup.rs).
- **Linux.** It's developed and tested on Linux. The launcher (`cargo drift`) finds and stops trainers through `/proc` and Unix signals, so it won't work elsewhere. The viewer and trainer may run on other systems but haven't been tried.
- **System libraries** for the window (macroquad) and gamepad support (gilrs):
  - Debian / Ubuntu: `sudo apt install pkg-config libx11-dev libxi-dev libgl1-mesa-dev libudev-dev`
  - Arch: `sudo pacman -S pkgconf libx11 libxi mesa systemd-libs`
- **ffmpeg** (optional): only needed for exporting replays as MP4.
- A gamepad is recommended for driving yourself. The keyboard works but steering is on/off.

## Quick start

```sh
git clone https://github.com/RakLord/drift.git
cd drift
cargo drift
```

`cargo drift` opens a menu for everything: start, resume, watch and stop training runs, drive saved tracks, and export videos. It builds what it needs on first use. Trainers run in the background and log to `runs/<run>/train.log`, so you can close the menu and come back later.

To just drive:

```sh
cargo run -p viewer
```

Press `H` in the viewer for the full list of keys on each screen.

## Driving controls

| Control | Keyboard | Gamepad |
|---|---|---|
| Steer | `A` / `D` or `←` / `→` | Left stick |
| Throttle | `W` or `↑` | Right trigger |
| Brake | `S` or `↓` | Left trigger |
| Handbrake | `Space` | `A` (south) or right bumper |
| Clutch | `Shift` | `X` (west) or left bumper |
| Reset / restart | | Start / Select |

There are no driver aids: no ABS, no steering assist and no auto-clutch. The car can stall.

## Training

The easiest way is `cargo drift` → **Start new training…**. To run the trainer directly:

```sh
cargo run --release -p train -- --name myrun --generations 500
```

Useful flags:

| Flag | What it does |
|---|---|
| `--resume` | Continue `runs/<name>/` from its last checkpoint |
| `--generations N` | Stop after generation N |
| `--stage N` | Start at curriculum stage N |
| `--track <file>` | Train on this saved track every generation (repeatable) |
| `--pool <file>` / `--pooled N` | Draw N tracks per generation from this pool |
| `--generated N` | Add N freshly generated tracks per generation |
| `--seed N` | Fix the random seed |

Each run writes to `runs/<name>/`:

- `metrics.csv`: per-generation stats (shown as charts in the viewer)
- `best.genome`: the best driver so far
- `top.genomes`: the top drivers, raced together by `--watch`
- `checkpoints/`: for `--resume`
- `replays/`: recordings of the best driver every few generations
- `tracks.csv`: which tracks each generation trained on
- `config/`: a snapshot of the config the run started with

Training is tuned through `config/training.toml` (population, curriculum stages, fitness weights) and `config/scoring.toml` (how drifts are judged). The car itself is `config/car.toml`. The viewer hot-reloads the car, scoring and track files while it's open.

## Watching and reviewing runs

| Command | What it does |
|---|---|
| `cargo run -p viewer -- --genome runs/<run>/best.genome` | Watch one trained driver |
| `cargo run -p viewer -- --follow <run>` | Watch a run live; picks up each new best |
| `cargo run -p viewer -- --watch <run>` | Race the run's top drivers together |
| `cargo run -p viewer -- --review <run>` | Pick drivers on a 4×3 grid for the trainer to boost |
| `cargo run -p viewer -- --charts --follow <run>` | The run dashboard (also `C` in the viewer) |
| `cargo run -p viewer -- --replays <run>` | Browse a run's replays |
| `cargo run -p viewer -- --replay <file> --video out.mp4` | Export a replay to MP4 (needs ffmpeg) |
| `cargo run -p viewer -- --tracks` | The track screen: browse, drive and pick training tracks |

Add `--track tracks/<file>.toml` to drive or watch on a specific saved track, or `--seed N` for a generated one. While an AI drives, `V` opens a live view of its network.

## Tracks

Saved tracks live in `tracks/` as TOML. A few are hand-made and inspired by real drift circuits (Ebisu, Meihan, Nikko, Driftland); the rest are generated. In the viewer, `N` makes a new generated track, `F` opens the generator settings and `K` keeps the current one.

## Project layout

| Crate | What it is |
|---|---|
| `common` | Vector maths, RNG, config loading and hot-reload |
| `sim` | Car physics, tracks, track generator, sensors, scoring. No graphics, deterministic, 120 Hz |
| `nn` | The neural network library |
| `agent` | The network driver and its file formats (genomes, replays) |
| `evo` | The genetic algorithm: islands, crossover, mutation, selection, curriculum |
| `train` | The headless trainer |
| `viewer` | The visual app: driving, watching, replays, review, dashboards |
| `launcher` | The `cargo drift` menu |

Tests: `cargo test --workspace`.

## Licence

[PolyForm Noncommercial 1.0.0](LICENSE). Free for personal use, research, hobby projects, education and non-profits. For commercial use, get in touch.
