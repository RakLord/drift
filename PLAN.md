# Drift AI — Plan

A top-down 2D drift game where cars are driven by small neural networks trained with a genetic algorithm (neuroevolution). Mostly our own code: physics, NN, and GA are written from scratch.

## Status

**Done: M0** (except a gamepad tuning session)
- `common`: Vec2/angle math, PCG32 RNG, TOML config + hot-reload watcher
- `sim/car`: full car physics driven by `config/car.toml`, plus 12 behaviour tests (`sim/tests/physics.rs`)
- `viewer`: free-drive screen with gamepad/keyboard, telemetry, smoke, skid marks, live car.toml reload

**Next: M1**, starting with Bezier tracks + walls + collisions, then the generator, zones, scoring, driver prep, game HUD.

**Conventions**
- The sim uses `f64` (SI units). The NN will use `f32`.
- World axes: +x right, +y up, angles CCW. Car frame: +x forward, +y left. Positive steer = left.
- Drift angle (`body_slip_angle`) is negative in a left-hand drift.
- macroquad's `Camera2D` is y-down by default, so `FollowCamera` uses a negative y zoom.
- The sim ticks at 120 Hz with 4 car substeps (~3 µs per car per tick).
- Effects are viewer-only, run on sim ticks, and use their own RNG.

**Commands**
- `cargo run -p viewer`: drive (`H` in-app for controls)
- `cargo test -p sim --test physics -- --nocapture --test-threads=1`: physics numbers
- `cargo run -p viewer -- --screenshot out.png --frames 420 --autopilot`: render a scripted drift to PNG (dev check for visuals)

**Tuning notes**
- It feels a little icy on keyboard. Retest on gamepad before tuning.
- Knobs: `tyre.slide_ratio` ↑, `tyre.falloff` ↑, `tyre.mu_peak` ↑, `steering.rate_deg_per_s` ↓.
- The single fixed gear makes 0–100 slow (~7.9 s) and limits angle at high speed. A gearbox is a stretch goal.

## Base concepts

- **Game:** drift a car around a judged course. Points only while drifting. Scoring follows Formula Drift judging: **Line** (clip points / zones), **Angle**, **Style**, with speed as a multiplier.
- **Combo:** points build up during a drift and are banked only on a clean exit. Spin-outs and wall hits lose the combo and add a penalty.
- **Driver prep:** before a run, the track is analysed into a list of corners. While driving, the car "knows" the next few corners before it can see them, so it can pre-plan entries.
- **Agent:** a fixed-size neural net reads sensors (raycasts, car state, driver prep) and outputs controls.
- **Training:** a genetic algorithm evolves the network weights, guided by a shaped fitness function and periodic human review. There is no backprop in v1.
- **Aesthetic:** JDM night style (sodium lights, neon, katakana HUD, coloured tyre smoke) on a Formula Drift-style course.

## Design decisions

| Area | Decision |
|---|---|
| Language | Rust |
| Rendering | macroquad, top-down 2D |
| Deps | macroquad, rayon, serde + toml (saves, config), gilrs (gamepad). Everything else is ours. |
| Sim | Pure Rust with no graphics deps, deterministic, fixed 120 Hz timestep |
| Physics | Quality is a top priority. See *Physics* |
| Decisions | NN runs every N ticks (default 4 → 30 Hz) and holds actions in between; cars are staggered |
| Visuals | Viewer-only: tyre smoke, skid marks, camera, HUD. Effects never touch the sim |
| Multi-car | Trainer runs headless; the viewer re-sims the top 10–50 genomes live, overlaid on one track |
| Replays | Stored as state frames (survive physics changes), interpolated for smooth playback |
| Hardware | Dev: Ryzen 7 3700X (8c/16t), 32 GB, RTX 3060. CPU-only training. A stronger PC comes later |

## Physics

Good driving and drift physics is what gives this project value. The GA will also find and exploit **any** physics bug (energy gain, low-speed jitter, wall tunnelling), so robust physics doubles as anti-cheating.

**Model:**
- **Wheels:** 4 wheels, each with its own spin speed, driven by drive/brake torque against the tyre force. This is what makes slip ratio, wheelspin and clutch kicks real.
- **Tyres:** simplified Pacejka curve with combined slip (friction circle/ellipse) and load sensitivity. Low-speed blending and relaxation keep them stable near standstill.
- **Weight transfer:** a spring-damper per corner for roll and pitch, so the load shift has a realistic delay. That's what makes weight-transfer initiations (feints) work.
- **Drivetrain:**
  - engine torque curve, rev limiter, engine braking and flywheel inertia
  - analog clutch that slips under speed difference
  - single fixed ratio (no gearbox in v1)
  - locked or welded diff, as on real drift cars
- **Steering and brakes:** high-lock steering (~60°) with a rate limit; front-biased foot brake; hydraulic handbrake on the rears.
- **Resistance:** aero drag and rolling resistance.
- **Integration:** 120 Hz tick, with tyre/wheel substeps if they turn out to be needed for stability (we'll measure).

**Tuning:**
- All car parameters live in a config file that reloads live while the viewer is running.
- The default car is a typical drift build: ~1250 kg, ~50/50 weight split, ~350 hp, welded diff.

**Validation (the gate before any AI work):**
1. **Feel test:** you drive it on a gamepad and can initiate, hold, link and recover drifts. Analog input matters here; keyboard isn't enough.
2. **Behaviour tests (automated):** each checks against plausible real-car numbers.
   - skidpad lateral g
   - 0–100 time
   - 100–0 braking distance
   - handbrake → rotation
   - clutch kick → wheelspin
   - a held steady-state drift on a circle with counter-steer
   - a car at rest stays at rest
3. **Telemetry overlay:**
   - per-wheel friction circles and slip vectors
   - loads
   - RPM
   - live graphs

## Tracks

- **Format:** a closed loop of cubic Bezier segments with smooth (mirrored) handles, plus a width, generated into inner/outer wall polylines. A grid speeds up ray and collision queries.
- **Procedural generator (v1):**
  1. Place random control points around a loop.
  2. Derive smooth handles.
  3. Reject tracks that self-intersect or have corners that are too tight.
  4. Auto-place zones: clip points at apexes, outer zones on long corners.
  5. Tracks can also be run mirrored or reversed.
- **Track library:** about 15 approved tracks, saved to disk. A good generated track can be approved into the library. Some library tracks are held out as **test tracks** that are never trained on.
- **Bezier editor:** later, not v1.

## Driver prep

- **Before the run:** our own code (not learned) analyses the track into a corner list. Each corner records:
  - direction, total angle, radius, length
  - clip point and zone positions
  - whether it links straight into the next corner as a transition
- **During the run:** the network gets the **next 2–3 corners** as inputs (distance + shape + zones).
- **Viewer:** the run opens with a camera fly-through of the course, and corners pop up in the HUD.
- **Stretch:** a learned version, where an evolved network writes its own driver prep from the raw track.

## Network

**Shape:** varies per island (e.g. `102 → 32 → 32 → 5`, tanh hidden layers).

### Inputs (102)

All inputs are in the car's own frame (x forward, y left) and scaled to roughly -1..1 with fixed constants, using tanh for unbounded values. World position/heading, lap progress, and elapsed time are deliberately left out, so the network can't memorise a track.

| Group | # | Inputs |
|---|---|---|
| Rays | 48 | 360°: 32 across the front 180°, 16 across the rear. Range 60 m. Value = `1 - dist/range`. Walls only |
| Body dynamics | 8 | forward vel, sideways vel, speed, drift angle as sin + cos, yaw rate, forward + sideways g-force |
| Wheels (×4) | 16 | per wheel: slip angle, slip ratio, grip usage (≥1 = sliding), load ÷ static load |
| Car state | 6 | applied steer, throttle, brake, handbrake, clutch; engine RPM |
| Track position | 4 | heading vs track direction as sin + cos, offset from centreline (-1..1), signed curvature here |
| Driver prep (×3 corners) | 18 | per corner: distance to entry, progress through it, signed total angle, sharpness, clip point position (-1 = none), outer zone coverage |
| Drift state | 2 | drifting (0/1), in a zone (0/1) |

**Sensor noise:** small Gaussian noise on inputs during training, for robustness. It's seeded per episode, so replays still match exactly. It can be turned off for test-track evaluation.

### Outputs (5)

Outputs are produced once per decision and held until the next one.

| Output | Activation | What it does |
|---|---|---|
| Steer | tanh (-1..1) | target steering angle as a fraction of full lock; the wheels move toward it at a limited rate |
| Throttle | sigmoid (0..1) | engine power |
| Brake | sigmoid (0..1) | foot brake, front-biased. Separate from throttle so left-foot braking is possible |
| Handbrake | sigmoid (0..1) | rear brake strength; used to start or extend a drift |
| Clutch | sigmoid (0..1) | clutch pedal (1 = fully disengaged). A clutch kick = pedal in on throttle, revs rise, sharp release → wheelspin |

## Training

- **Genome:** the network's weights as a flat list.
- **Generation loop:**
  1. Evaluate every genome on the same K tracks (fresh procedural tracks each generation).
  2. Aggregate fitness across the tracks.
  3. Elites survive unchanged.
  4. The rest are bred via tournament selection, crossover and mutation.
- **Islands:** several sub-populations, each with its own network shape and activation functions. Breeding happens within an island only. Periodically the worst island is replaced with a copy of the best.
- **Crossover:** neuron-level (a child takes each hidden neuron's whole incoming weight set from one parent).
- **Mutation:** Gaussian noise on a random subset of weights, plus a rare large reset. σ shrinks over time or self-adapts per genome.
- **Fitness (shaped curriculum):** progress → +Angle → +Line → +Style, then the pure game score dominates.
- **Early termination:** a crash, spin, or no progress for a few seconds ends the episode.
- **Diversity:** small tournaments, a few random immigrants per generation, and a tracked diversity metric.
- **Polish phase:** smaller σ, more tracks per evaluation, and elites re-evaluated each generation.
- **Monitoring:** see *Network view & metrics*.

## Human review

- **When:** every N generations (default 25), training pauses for review. It auto-continues if there's no input after a timeout.
- **What:** a 5×5 grid of 25 synced replays on one track (random from the library, or picked by you). Roughly 15 top scorers plus 10 interesting outliers.
- **Controls:** click a tile for fullscreen; speed control; mark each car ★ love / ✓ good / ✗ bad (✗ usually means a scoring exploit to fix).
- **How ratings feed back:**
  1. ★ genomes are protected as elites and get extra breeding slots.
  2. Their descendants get a fitness bonus that fades over generations.
  3. *(Later)* A learned style critic: ratings + run stats are fitted to a small model that becomes the automatic Style score.

## Network view & metrics

**Network view (per genome):**
- Nodes in layer columns. Inputs are grouped and labelled (rays, dynamics, wheels, driver prep); rays can also be drawn as a radial fan around a mini car.
- Edges are coloured by sign and their thickness shows weight size. Only the strongest edges are shown by default (adjustable threshold) so it stays readable.
- **Live mode:** during a replay, nodes light up with their current activation and the outputs show as bars (steer, throttle, brake, handbrake).
- **Info panel:**
  - island and network shape
  - fitness, with the Line/Angle/Style breakdown
  - generation born and parents
  - ★/✓/✗ history
- Open it from any car: the overlay view, a review grid tile, or a replay.

**Generation metrics:**
- **Fitness:** best / mean / median / worst, overall and per island
- **Score breakdown:** Line / Angle / Style of the best car and the population average
- **How runs ended:** % crash, spin, no progress, timeout, finished
- **Driving stats:**
  - % of time spent drifting
  - average drift angle
  - laps completed
  - zones hit
- **Health:**
  - diversity
  - mutation σ
  - generation time
  - sim steps per second
- **Generalisation:** best genome's score on the held-out test tracks, every N generations
- **Shown as:** a dashboard panel in the viewer, with graphs over generations and human-review points marked. The trainer also writes a per-generation CSV log.

## Code structure & modularity

### Rules
- **One concept per file.** Keep files small (aim for under ~400 lines) and split a file when it grows past one job.
- **Dependencies only point one way** (see graph). `sim` never knows about networks, the GA or graphics. `nn` never knows about cars.
- **Thin interfaces between parts:**
  - `sim::Controller`: anything that drives a car (human gamepad, neural net, scripted test driver) implements `decide(observation) -> Controls`.
  - `nn` exposes flat params, so `evo` treats genomes as plain lists of numbers.
- **One source of truth:**
  - The input layout (names and index ranges) is defined once, in `sim/sensors/observation.rs`. The network view uses it for its labels.
  - Normalisation constants live in one module.
- **Tunables live in config files,** not scattered constants: car params, training settings, track generator settings.
- **Determinism:**
  - seeded RNG only
  - no wall-clock time in the sim
  - no iteration order that depends on hash maps
- **Versioned save formats:** genome, track and replay files carry a version plus the network shape and input layout, so old files fail loudly instead of silently misbehaving.
- **Tests sit next to the code** (unit tests). Physics behaviour tests live in `sim/tests/`.

### Dependency graph
```
common  ← sim, nn
nn      ← evo, agent
sim     ← agent
agent   ← train, viewer      (agent = NN driver: plugs a network into sim::Controller)
evo     ← train, viewer
```

### File structure
```
drift/
  Cargo.toml                 workspace
  PLAN.md
  config/                    car.toml, training.toml, generator.toml (hot-reloaded where useful)
  assets/                    fonts, textures, shaders
  tracks/                    track library (approved + test tracks)
  runs/                      (gitignored) checkpoints, metrics CSV, replays, ratings
  crates/
    common/src/              math (Vec2, angles), rng (PCG), config loading
    sim/src/
      car/                   params, state, wheel, tyre, suspension, engine (+clutch),
                             drivetrain (diff), steering, step (integration)
      track/                 bezier, geometry (walls), grid, zones, generator, library (load/save)
      sensors/               rays, driver_prep, observation (input layout + normalisation), noise
      scoring/               drift (detection), judge (line/angle/style), combo
      controls.rs            the 5 outputs
      controller.rs          Controller trait
      episode.rs             run one car on one track, termination rules
      frame.rs               replay state frame
    nn/src/                  matrix, activation, layer (dense), network (sequential), init
    agent/src/               nn_driver (Network → Controller), genome ↔ network mapping
    evo/src/                 genome, population, island, selection, crossover (neuron-level),
                             mutation, fitness (curriculum), review (ratings → selection), diversity
    train/src/               main, config, runner (parallel eval), checkpoint, metrics (CSV),
                             publish (send genomes to viewer)
    viewer/src/
      main.rs, app.rs        screen state machine
      input/                 gamepad, keyboard
      screens/               play, watch (live overlay), replay, review (5×5 grid), dashboard
      render/                camera, car, track, hud, theme (JDM palette)
        effects/             particles, smoke, skidmarks, post
      debug/                 telemetry, rays, network_view
```

## Milestones

1. **M0:** car physics, gamepad driving, telemetry overlays, basic smoke + skid marks. **Must pass the physics validation gate before M2**
2. **M1:** Bezier tracks + generator, zones, scoring, driver prep, HUD. A playable game
3. **M2:** `nn` + `evo` (islands) + headless trainer + per-generation CSV metrics; learn to drive laps
4. **M3:** drift fitness curriculum, multi-track training, track library + test tracks
5. **M4:** live multi-car viewer, replays, human review grid, network view, metrics dashboard, aesthetic polish
6. **Stretch:**
   - gearbox (+ gear outputs)
   - Bezier track editor
   - learned style critic
   - learned driver prep
   - tandem battles
   - NEAT/ES
   - autograd
