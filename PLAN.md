# Drift AI — Plan

A top-down 2D drift game where cars are driven by small neural networks trained with a genetic algorithm (neuroevolution). Mostly our own code: physics, NN, and GA are written from scratch.

## Status

**Done: M0** (except a gamepad tuning session)
- `common`: Vec2/angle math, PCG32 RNG, TOML config + hot-reload watcher
- `sim/car`: full car physics driven by `config/car.toml`, plus 12 behaviour tests (`sim/tests/physics.rs`)
- `viewer`: free-drive screen with gamepad/keyboard, telemetry, smoke, skid marks, live car.toml reload

**Done: M1**
- Done:
  - Bezier tracks (`sim/track`, `tracks/test_loop.toml`), hard/soft edges, verge grip
  - wall collisions (`sim/world`)
  - track rendering, sparks, cosmetic bumper damage
  - zones and scoring (`sim/scoring`, `config/scoring.toml`), score HUD with popups
  - corner analysis (`track/corners.rs`) and driver prep (`sim/sensors/driver_prep.rs`)
  - procedural drift-track generator with figure-8 crossroads (`track/generator/`, `track/crossings.rs`, `config/generator.toml`)
  - viewer: `N` new track, `F` generator panel (sliders, save as defaults), `K` keep track, `M` map view
  - track, car, scoring and generator files all hot-reload in the viewer

**Done: M2** (a run reached ~0.94 laps of unseen library tracks by gen 60)
- Done:
  - `sim`: observation (111 inputs, layout in `sensors/observation.rs`), `Controller` trait, `episode.rs` (rolling start, early termination, stats)
  - `nn` (own network library), `agent` (network driver, `.genome` files), `evo` (islands, neuron-level crossover, self-adaptive mutation, curriculum)
  - `train`: headless trainer with `config/training.toml`, `runs/<name>/` (metrics.csv, best.genome, checkpoints, config snapshot)
  - viewer `--genome` watch mode (`Tab` take over, `I` inputs panel)
  - live follow: `viewer --follow <run>`
  - clutch heat/fade and stalling (stops the AI riding the clutch), with clutch metrics in the trainer
  - network view (`V`, pulled forward from M4): live layers, edges, inspect, neuron health
  - drift shaping from gen 0, Pareto selection, stage-change reseed, behaviour diversity and drift metrics (after `runs/1` learned to grip-drive)

**In progress: M3**
- Done: library tracks carry a `test` flag (on unless `test = false`, e.g. the skidpad); the trainer tests only on those.
- Done: run dashboard in the viewer (`C` / `--charts`): fitness, drift quality, angle, pedals, diversity, end reasons, from `metrics.csv`.
- Done: library panel (`O` / `--library`): every track in `tracks/`, click to drive, toggle the test set, rename, delete (moved to `tracks/deleted/`). `L` cycles the library.
- Done: polish phase (`polish` on a stage) and a sturdier best.genome (`[best]`: top 5 re-run on 6 extra tracks).
- Next: M3's features are in; what's left is user-led training to get cars drifting through drive -> slide -> judged (and, once judged works, polishing).

**In progress: M4** (network view and run dashboard already done)
- Done: replays (see *Replays*): the trainer records the saved best every `replays.every` gens; the viewer plays them back (`--replays <run>`), and `X` saves the last AI run.
- Next: live multi-car viewer, then the human review grid, then visual polish.

**Conventions**
- The sim uses `f64` (SI units). The NN will use `f32`.
- World axes: +x right, +y up, angles CCW. Car frame: +x forward, +y left. Positive steer = left.
- Drift angle (`body_slip_angle`) is negative in a left-hand drift.
- macroquad's `Camera2D` is y-down by default, so `FollowCamera` uses a negative y zoom.
- The sim ticks at 120 Hz with 4 car substeps (~3 µs per car per tick).
- Effects are viewer-only, run on sim ticks, and use their own RNG.

**Commands**
- `cargo drift`: menu launcher for all of the below (start/resume/stop/watch runs, drive tracks). Trainers run in the background, logging to `runs/<run>/train.log`
- `cargo run -p viewer`: drive (`H` in-app for controls)
- `cargo test -p sim --test physics -- --nocapture --test-threads=1`: physics numbers
- `cargo run -p viewer -- --screenshot out.png --frames 420 --autopilot`: render a scripted lap to PNG (dev check for visuals)
- `cargo run -p viewer -- --seed 7 --map`: open generated track 7 in map view
- `cargo run --release -p train -- --name <run> [--generations N] [--stage N] [--track path]... [--generated N] [--resume]`: train
- `cargo run -p viewer -- --genome runs/<run>/best.genome`: watch a trained driver
- `cargo run -p viewer -- --follow <run>`: watch a run live (picks up each new best between runs)
- `--track tracks/<file>.toml` on any viewer command: drive/watch on that saved track instead of `test_loop`
- `V` while an AI drives: network view (add `--net` to start with it open)
- `cargo run -p viewer -- --replays <run>`: watch a run's replays (newest first, picker open); `--replay <file>` for one file; `X` while an AI drives saves its last run

**Tuning notes**
- It feels a little icy on keyboard. Retest on gamepad before tuning.
- Knobs: `tyre.slide_ratio` ↑, `tyre.falloff` ↑, `tyre.mu_peak` ↑, `steering.rate_deg_per_s` ↓.
- The single fixed gear makes 0–100 slow (~7.9 s) and limits angle at high speed. A gearbox is a stretch goal.

## Base concepts

- **Game:** drift a car around a judged course. Points only while drifting. Scoring follows Formula Drift judging: **Line** (clip points / zones), **Angle**, **Style**, with speed as a multiplier.
- **Combo:** points build up during a drift and are banked only on a clean exit. Spin-outs and hard wall hits lose the combo and add a penalty.
- **Wall taps:** a light, controlled rear-bumper tap on a hard wall (e.g. in an outer zone) earns a **Style bonus**. This is JDM drift.
- **Driver prep:** before a run, the track is analysed into a list of corners. While driving, the car "knows" the next few corners before it can see them, so it can pre-plan entries.
- **Agent:** a fixed-size neural net reads sensors (raycasts, car state, driver prep) and outputs controls.
- **Training:** a genetic algorithm evolves the network weights, guided by a shaped fitness function and periodic human review. There is no backprop in v1.
- **Aesthetic:** JDM night style (sodium lights, neon, katakana HUD, coloured tyre smoke) on a Formula Drift-style course.

## Design decisions

| Area | Decision |
|---|---|
| Language | Rust |
| Rendering | macroquad, top-down 2D |
| Deps | macroquad, rayon, serde + toml (saves, config), gilrs (gamepad). Everything else is ours. Exception: the `launcher` tool uses `inquire` for its menus |
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
  - analog clutch with a realistic pedal: free play (0–30%), a bite zone (30–75%), then disengaged. It **heats up** when it slips and a hot clutch fades (riding it in the bite zone cooks it, a kick barely warms it)
  - no anti-stall: the engine **stalls** if held under 500 rpm for ~0.4 s (a quick handbrake yank in gear recovers, holding it stalls); `Y` / pad Select restarts
  - single fixed ratio (no gearbox in v1)
  - locked or welded diff, as on real drift cars
- **Steering and brakes:** high-lock steering (~60°) with a rate limit; front-biased foot brake (full pedal just under lock-up in a straight line); hydraulic handbrake on the rears. No driver aids: no ABS, no steering assist, no auto-clutch.
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

- **Format:** a closed loop of cubic Bezier segments with smooth (mirrored) handles, plus a width. It's built into a sampled centreline (distance along track, direction, curvature) and left/right edge polylines. A grid speeds up ray and collision queries.
- **Edges:** each side of each section is **hard** or **soft**.
  - **Hard:** a barrier (concrete or tyre wall) that you collide with.
  - **Soft:** a grass/gravel verge (~6–10 m) with no collision; tyres get ~0.6× grip and more rolling resistance. A hard fence sits beyond the verge.
  - The world looks up the surface under each tyre and passes per-wheel grip factors to the car, so the car never knows about tracks.
- **Wall impacts:** rigid-body impulse at the contact point, with near-zero bounce and scraping friction capped by the impact.
  - Overlap is fixed by correcting position (never adds energy).
  - Light taps only nudge the car; hard hits spin it.
  - Impacts emit contact events (point, normal, speed) for sparks, debris, and cosmetic bumper damage (bumpers rip off).
- **Procedural generator:** builds drift-style tracks from corner pieces, then closes the loop with the shortest smooth path back to the start.
  - The defaults are tuned for compact, constantly turning courses: no straight over ~60 m, about 10 corners per km, figure-8s by default.
  - When a seed can't produce a valid track, the generator reports the most common rejection reasons, with the setting to change for each.
  - **Pieces:** hairpin, sweeper, S-bend (transition), decreasing radius, chicane/manji, kink, with straights between.
  - **Figure-8s:** tracks may cross themselves as **flat crossroads**. Boundaries are opened where they'd sit on the other road. Crossings must be ≥ 35°; parallel overlaps are rejected.
  - **Edges and zones** are assigned from the corner analysis, with no zones at crossroads.
  - **Tuning:** everything is in `config/generator.toml`, and editable live in the viewer's generator panel (`F`), which can save it back.
- **Track library:** about 15 approved tracks, saved to disk. A good generated track can be approved into the library (`K`). Library tracks are never trained on; those with `test` on (the default) form the **test set** the trainer measures generalisation on.
- **Bezier editor:** later, not v1.

## Scoring

All numbers live in `config/scoring.toml` (hot-reloaded).

- **Zones** sit in the track file, positioned in section coordinates (anchor index + fraction), so they stay with their corner when anchors move.
  - **Clip point** (inside edge): scored on the **front bumper**'s closest approach while drifting. Pays out once per lap.
  - **Outer zone** (outside edge): scored on the **rear bumper**'s proximity to the edge, per second, while drifting through it.
- **Drifting:** angle ≥ 12°, speed ≥ 25 km/h, moving forwards along the track. The drift ends after 0.3 s below 8° (a clean exit). Over 100° is a spin.
- **Points per second while drifting**, × a speed multiplier (0.5× at 30 km/h → 1.5× at 100 km/h):
  - **Angle:** ramps up from 12°, full at 40–65°, tapers off towards a spin.
  - **Line:** clips + outer zones.
  - **Style:**
    - transitions (linking direction changes) are a bonus
    - wall taps are a bonus: rear bumper into a hard wall at 0.3–3 m/s; at most one per 2 s and 2 per combo, so they can't be farmed
    - bobbles (angle dips sharply then recovers) are a penalty
- **Combo:** points pile up × a multiplier.
  - The multiplier gets +0.1× per zone, transition or tap, capped at 3×.
  - It's banked on a clean exit.
  - A spin or a hard wall hit (≥ 3.5 m/s) loses it and adds a penalty.
  - All four wheels on the verge pauses scoring (no points, combo kept).
- The **Line/Angle/Style breakdown** is kept separately for metrics and fitness.

## Driver prep

- **Before the run:** our own code (not learned) analyses the track into a corner list. Each corner records:
  - direction, total angle, radius, length
  - clip point and zone positions
  - whether it links straight into the next corner as a transition
- **During the run:** the network gets the **next 2–3 corners** as inputs (distance + shape + zones).
- **Viewer:** the run opens with a camera fly-through of the course, and corners pop up in the HUD.
- **Stretch:** a learned version, where an evolved network writes its own driver prep from the raw track.

## Network

**Shape:** varies per island (e.g. `111 → 32 → 32 → 5`, tanh hidden layers).

### Inputs (111)

All inputs are in the car's own frame (x forward, y left) and scaled to roughly -1..1 with fixed constants, using tanh for unbounded values. World position/heading, lap progress, and elapsed time are deliberately left out, so the network can't memorise a track.

| Group | # | Inputs |
|---|---|---|
| Rays | 48 | 360°: 32 across the front 180°, 16 across the rear. Range 60 m. Value = `1 - dist/range`. Hit the track edge (hard or soft) |
| Body dynamics | 8 | forward vel, sideways vel, speed, drift angle as sin + cos, yaw rate, forward + sideways g-force |
| Wheels (×4) | 16 | per wheel: slip angle, slip ratio, grip usage (≥1 = sliding), load ÷ static load |
| Car state | 7 | applied steer, throttle, brake, handbrake, clutch; engine RPM; clutch temperature |
| Track position | 6 | heading vs track direction as sin + cos, offset from centreline (-1..1), signed curvature here, left/right edge hard (0/1) |
| Driver prep (×3 corners) | 24 | per corner: distance to entry, progress through it, signed total angle, sharpness, clip point position (-1 = none), outer zone coverage, inside edge hard, outside edge hard |
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
  1. Evaluate every genome on the same K tracks: fresh procedural tracks each generation (`tracks.per_generation`), plus any saved tracks listed in `tracks.fixed`. Fixed tracks are left out of the test set.
  2. Aggregate fitness across the tracks.
  3. Elites survive unchanged.
  4. The rest are bred via tournament selection, crossover and mutation.
- **Islands:** several sub-populations, each with its own network shape and activation functions. Breeding happens within an island only. Periodically the worst island is replaced with a copy of the best.
- **Crossover:** neuron-level (a child takes each hidden neuron's whole incoming weight set from one parent).
- **Mutation:** Gaussian noise on a random subset of weights, plus a rare large reset. σ shrinks over time or self-adapts per genome.
- **Fitness (shaped from gen 0):** driving reward (progress, capped after stage 1, plus time alive) multiplied by (1 + drift_bonus × drift quality), where drift quality is raw angle-time. Banked judged score (Line/Angle/Style) counts in the final stage. Stages: drive → slide → judged. Shaping stages soften the scoring (drift from 5°, flat speed multiplier, spins don't end the run, no bobble penalty).
- **Clutch penalty (`clutch` per stage):** a press shorter than 0.4 s is a kick and is free. Holding the clutch any longer counts, each second weighted by drift angle (0.25 with no angle, rising to 1 at 45°). The run's reward and both Pareto objectives are scaled by 1 − strength × (weighted hold time ÷ run time), never below 0. Strength 2 in every stage. Logged as `clutch_hold` / `clutch_hold_angle`.
- **Selection:** Pareto ranking over (driving, drift quality) inside each island, so slidey crashers survive alongside grip drivers.
- **Stage change:** σ resets and 30% of each island is reseeded with heavily mutated copies of its best.
- **Starting priors:** calm start (straight, some throttle, brake off, handbrake/clutch light).
- **Early termination:** a crash, spin, or no progress for a few seconds ends the episode.
- **Diversity:** small tournaments, a few random immigrants per generation; diversity is tracked in behaviour space (lap share, drift share, angle, handbrake, clutch) as well as weight space.
- **Polish phase:** an optional `polish` block on a curriculum stage: smaller σ (reset on entry, no reseed), no immigrants, more generated tracks per generation. Every genome, elites included, is already re-evaluated on fresh tracks each generation.
- **Saved best (`[best]`):** the top few genomes by fitness also drive some extra generated tracks; the best over all its runs is saved as best.genome, judges curriculum advance and runs the test set, so a lucky draw can't replace a steadier driver.
- **Monitoring:** see *Network view & metrics*.

## Replays

- **File (`.replay`, `agent::replay_file`):** magic + version, a TOML header, then binary data.
  - Header: provenance (run, generation, island, stage, fitness, result), the track definition, car and scoring params (embedded, so a replay survives library edits and config changes), the frame field names, and the frame rate.
  - Frames at 30 Hz (`sim::frame`): pose, velocity, yaw rate, steer, accel; per wheel spin, load, force, slip ratio/angle, combined slip, slip speed; rpm, clutch temp/lock, limiter, engine running; controls; progress; banked score and combo. Read by field name, so adding a field later still loads old files (missing = 0).
  - Sparse records with their tick: wall contacts (sparks, damage), score events (popups), zones lighting up.
  - The network's inputs at every decision, plus the genome, so the network view (`V`) replays exactly (off if the input layout has changed since).
- **Recorded by:**
  - the trainer: every `replays.every` generations (default 10), the saved best re-run on each of that generation's tracks (identical to its scored run), into `runs/<run>/replays/`
  - the viewer: `X` saves the last AI run (AI runs only).
- **Playback:** interpolated between frames; smoke, skids and sparks regenerate from the recorded wheel states and contacts. `--replay <file>` or `--replays <run>` (picker). Timeline with score-event ticks, click/drag to seek; Space play/pause, `,` `.` step, `[` `]` speed; the usual toggles. Seeking rebuilds the effects from the start.

## Human review

- **When:** every N generations (default 25), training pauses for review. It auto-continues if there's no input after a timeout.
- **What:** a 5×5 grid of 25 synced replays on one track (random from the library, or picked by you). Roughly 15 top scorers plus 10 interesting outliers.
- **Controls:** click a tile for fullscreen; speed control; mark each car ★ love / ✓ good / ✗ bad (✗ usually means a scoring exploit to fix).
- **How ratings feed back:**
  1. ★ genomes are protected as elites and get extra breeding slots.
  2. Their descendants get a fitness bonus that fades over generations.
  3. *(Later)* A learned style critic: ratings + run stats are fitted to a small model that becomes the automatic Style score.

## Network view & metrics

**Network view (per genome):** `V` while an AI drives cycles off → split (track left ~45%, network right) → fullscreen.
- **Layout:** input area, then one column per hidden layer, then the 5 outputs.
  - The 48 rays are shown as a fan of nodes around a mini car, at their real angles. The fan only displays values: the rays feed the network through one bundled "rays" node at the top of the input column.
  - The other inputs form a grouped, labelled column (body, wheels, car, track, prep, drift). Click a group header to collapse it to one node with bundled edges.
- **Nodes** are filled by their activation (pink +, cyan −).
- **Edges** show the live contribution `weight × source activation` by default, with raw weights as a toggle. Colour shows the sign; thickness shows size relative to the layer's largest. Only the strongest X% per layer are drawn (adjustable).
- **Inspect:**
  - hover a node to see its edges, label, value, pre-activation and bias
  - click to pin it, which lists its top contributors (e.g. "steer ← side vel −0.42")
- **Outputs** show a value bar and a ~5 s sparkline each.
- **Neuron health:** a rolling 5 s window marks **dead** neurons (never change) and **saturated** ones (pinned at the activation's limits), with counts per layer.
- While you've taken over (`Tab`) the view freezes.
- *(Later)* An info panel with island, fitness breakdown, parents and ratings, opened from any car in the overlay, the review grid or a replay.

**Generation metrics:**
- **Fitness:** best / mean / median / worst, overall and per island
- **Score breakdown:** Line / Angle / Style of the best car and the population average
- **How runs ended:** % crash, spin, no progress, timeout, finished
- **Driving stats:**
  - % of time spent drifting
  - average drift angle
  - laps completed
  - zones hit
  - pedal use: handbrake, clutch, clutch slip, clutch held past a kick
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
    launcher/src/            `cargo drift` menu: runs (status from runs/ + /proc), tracks, launch (build + spawn)
    viewer/src/
      main.rs, app.rs        screen state machine
      input/                 gamepad, keyboard
      screens/               play, watch (live overlay), replay, review (5×5 grid), dashboard
      render/                camera, car, track, hud, theme (JDM palette)
        effects/             particles, smoke, skidmarks, post
      debug/                 telemetry, rays, network_view/ (layout, edges, health, inspect, outputs)
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
