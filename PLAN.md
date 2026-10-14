# Drift AI — Plan

A top-down 2D drift game where cars are driven by small neural networks trained with a genetic algorithm (neuroevolution). Mostly our own code: physics, NN, and GA are written from scratch.

## Status

**Done: M0** (except a gamepad tuning session)
- `common`: Vec2/angle math, PCG32 RNG, TOML config + hot-reload watcher
- `sim/car`: full car physics driven by `config/car.toml`, plus 12 behaviour tests (`sim/tests/physics.rs`)
- `viewer`: free-drive screen with gamepad/keyboard, telemetry, smoke, skid marks, live car.toml reload

**Done: M1**
- Bezier tracks (`sim/track`), hard/soft edges, verge grip
- wall collisions (`sim/world`)
- track rendering, sparks, cosmetic bumper damage
- zones and scoring (`sim/scoring`, `config/scoring.toml`), score HUD with popups
- corner analysis (`track/corners.rs`) and driver prep (`sim/sensors/driver_prep.rs`)
- procedural drift-track generator with figure-8 crossroads (`track/generator/`, `track/crossings.rs`, `config/generator.toml`)
- viewer: `N` new track, `F` generator panel (sliders, save as defaults), `K` keep track, `M` map view
- track, car, scoring and generator files all hot-reload in the viewer

**Done: M2** (a run reached ~0.94 laps of unseen library tracks by gen 60)
- `sim`: observation (111 inputs, layout in `sensors/observation.rs`), `Controller` trait, `episode.rs` (rolling start, early termination, stats)
- `nn` (own network library), `agent` (network driver, `.genome` files), `evo` (islands, neuron-level crossover, self-adaptive mutation, curriculum)
- `train`: headless trainer with `config/training.toml`, `runs/<name>/` (metrics.csv, best.genome, checkpoints, config snapshot)
- viewer `--genome` watch mode (`I` inputs panel; its `Tab` take-over key was removed after run9)
- live follow: `viewer --follow <run>`
- clutch heat/fade and stalling (stops the AI riding the clutch), with clutch metrics in the trainer
- network view (`V`, pulled forward from M4): live layers, edges, inspect, neuron health
- drift shaping from gen 0, Pareto selection, stage-change reseed, behaviour diversity and drift metrics (after `runs/1` learned to grip-drive)

**In progress: M3**
- Done: library tracks carry a `test` flag (on unless `test = false`, e.g. the skidpad); the trainer tests only on those.
- Done: run dashboard in the viewer (`C` / `--charts`): four pages of charts from `metrics.csv`, with headline numbers and trend lines. See *Network view & metrics › Run dashboard*.
- Done: library panel (`O` / `--library`): every track in `tracks/`, click to drive, toggle the test set, rename, delete (moved to `tracks/deleted/`). `L` cycles the library. Now replaced by the track screen.
- Done: polish phase (`polish` on a stage) and a sturdier best.genome (`[best]`: top 5 re-run on 6 extra tracks).
- Done: track pool (`tracks.pool` + `pool_per_generation`): each generation draws a few saved tracks; the `[best]` check runs take pool tracks first. Set in `cargo drift`: when starting a run, or on resume ("Change its tracks?", saved into the run's config).
- Done: track screen (`O` / `--tracks`, see *Tracks › Track manager*): map cards, stats, fixed/pool/off roles for the defaults or a run, launcher "Pick in the viewer".
- Done: `runs/<run>/tracks.csv` and the "this generation" strip on the track screen.
- Done: Results tab (`Tab` on the track screen, with a run picked): per-track results over the last N generations, worst first, and the worst generated tracks to keep.
- Done: run8 prep (after run7): three Pareto objectives (driving, flow, judged score), a smoothness penalty on sawing the throttle and steering, and stricter judging against jiggles (angle credit from 20°, drifts under 1 s bank nothing, transitions need 1.5 s sides and pay by the angle held). See *Training* and *Scoring*.
- Done: steering as a rate for new runs (`[episode] steering`, see *Outputs*): the dash wheel showed run8 sawing its steer request ~4 times a second. Old runs keep position steering.
- Next: Generate tab (see *Tracks › Track manager*).
- Then: M3's features are in; what's left is user-led training to get cars drifting through drive -> slide -> judged (and, once judged works, polishing).

**In progress: M4** (network view and run dashboard already done)
- Done: replays (see *Replays*): the trainer records the saved best every `replays.every` gens; the viewer plays them back (`--replays <run>`), and `X` saves the last AI run.
- Done: video export (see *Replays › Video export*): `R` in a replay, `--video`, or the launcher's "Export video…" writes a small MP4.
- Done: live multi-car viewer (see *Live multi-car viewer*): `--watch <run>` races the run's `top.genomes` together.
- Done: driver dash (`render/hud.rs`): one bottom-centre panel with speed (mph), drift angle, tach, clutch temp, a steering wheel (turns with the steer request, ±270°; a cyan dot marks the actual rack) and clutch / brake / throttle / handbrake bars. On the play screen it sits above the help text.
- Done: human review (see *Human review*): `--review <run>` shows 12 of the run's drivers on a 4×3 grid; picked ones are sent to the trainer, which protects them for 50 gens and floods their island with their children.
- Done: night lighting and neon glow (see *Visual style*): street lamps placed from the track, headlight beams, tail and brake lights, glowing zones, sparks and combo, a vignette.
- Next: katakana HUD accents and coloured tyre smoke (the rest of *Visual style*).

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
- `cargo run --release -p train -- --name <run> [--generations N] [--stage N] [--track path]... [--pool path]... [--pooled N] [--generated N] [--resume]`: train
- `cargo run -p viewer -- --genome runs/<run>/best.genome`: watch a trained driver
- `cargo run -p viewer -- --follow <run>`: watch a run live (picks up each new best between runs)
- `--track tracks/<file>.toml` on any viewer command: drive/watch on that saved track instead of `generated_1` (the fallback when it's gone: `config/default_track.toml`, a copy built into the viewer)
- `V` while an AI drives: network view (add `--net` to start with it open)
- `cargo run -p viewer -- --watch <run>`: the run's top genomes racing live (`F` / click to follow, `O` leaderboard, `Tab` its details, `D` driver info, `U` review grid)
- `cargo run -p viewer -- --review <run>`: the review grid: pick drivers for the trainer to boost (`U` from watch or while following a run)
- While following a run (`--follow`): `Q` opens its top cars, `J` its replays (`Q` / `J` also switch between those two); Esc goes back. `cargo drift` → Runs → a run has the same two options
- `cargo run -p viewer -- --replays <run>`: watch a run's replays (newest first, picker open); `--replay <file>` for one file; `X` while an AI drives saves its last run
- `cargo run -p viewer -- --replay <file> --video <out.mp4 | folder/>`: record a replay to MP4 and exit. `--genome <path> | --follow <run>` with `--track <toml> | --seed N` instead drives that genome on the track first (saved as `replays/video_gNNNNN_<track>.replay`). `R` in the replay screen does the same for the open replay
- `cargo run -p viewer -- --tracks [--run <run>]`: the track screen (`O` from play; while following a run it opens on that run). `--set <file>` edits just that file's `[tracks]` (the launcher uses it)

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
- **Training:** a genetic algorithm evolves the network weights, guided by a shaped fitness function and human picks. There is no backprop in v1.
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

### Track manager (in progress: Library and Results tabs and the this-generation strip done)

Why: run6 sat at ~730 m for 280 gens with the gate value swinging ±80 m, mostly from which random generated tracks it drew (full-time runs ranged 387–1195 m). 9 of the saved driver's 10 runs per generation were fresh generated tracks nobody chose, and picking saved tracks in the launcher is a bare name list.

- **Where:** a full screen in the viewer (`--tracks`, and `O` from play mode); the launcher's Tracks menu opens it. It replaces the library panel.
- **Launcher:** when starting a run or changing a run's tracks, it offers "Pick in the viewer (maps)" beside the name lists. The viewer edits a temp `[tracks]` file (`--tracks --set <file>`), and the launcher reads it back and passes flags as before.
- **Layout:** a grid of track cards (map outline, name, length, corners, role chips) and a detail pane for the selected card (bigger map, stats, results, actions).
- **Stats** from the corner analysis: length, corners per km, sharpest corner, hairpins, figure-8 crossings, hard-edge share.
- **Tabs:**
  - **Library:** every `tracks/*.toml`. Drive, rename, delete, toggle test, set its role in the training set being edited.
  - **Generate:** a batch of generated tracks from `config/generator.toml`; reroll, filter by length/corners, keep the good ones into the library.
  - **Results** (`Tab`, only with a run picked): a run's per-track results from `tracks.csv` over a `LAST N GENS` window (10 / 20 / 50 / 100 / all, default 20; whole-run means are dragged down by early generations).
    - Library tracks, worst first by mean laps: runs, mean laps and metres, early-end share, most common end, a line per role, and a laps-per-generation chart over the whole run. Fixed / pool / off and drive work as on the Library tab.
    - Worst generated: generated tracks are driven once each, so these are the 24 generated runs in the window with the fewest laps, which can be kept into the library. They're built a few per frame, since generating takes tens of ms each.
- **Training set being edited**, picked at the top: *Defaults* (`config/training.toml`, for new runs) or a run (`runs/<run>/config/training.toml`, takes effect on `--resume`; warns if its trainer is running). Edits change only the `[tracks]` lines.
- **Roles** a library track can have in a training set:
  - **fixed:** driven every generation (as now).
  - **pool:** new. Each generation draws `pool_per_generation` of the pool, deterministic from (seed, generation), without repeats.
  - **off.**
  - Fresh generated tracks stay as `per_generation`. A run's test set stays "test-flagged library tracks it doesn't train on", so pool tracks leave it too.
- **Per-track results:** the trainer appends the saved driver's runs to `runs/<run>/tracks.csv` each generation: generation, track (library file or `gen:<seed>`), role (fixed / pool / generated / check / test), length, progress, laps, time, end. The screen shows per track: runs, mean progress and laps, early-end share and the most common end.
- **Decided:**
  - The `[best]` check tracks take pool tracks not drawn this generation first, then generated ones (one shuffle per generation, so they never overlap).
  - Generated tracks kept from Results are rebuilt from seed + the run's snapshot `generator.toml`; the trainer doesn't save every generated track.
  - A track can't be both fixed and pool (the trainer refuses).
- **This generation:** with a run picked, a strip shows every track its saved driver drove in the newest generation of `tracks.csv` (generated ones rebuilt from seed), each with its laps and end. Generated ones can be kept.
- **Build order:** pool mode (done) → screen with Library tab + training-set editing → `tracks.csv` + this-generation strip → Results tab (done) → Generate tab. The screen came first because not seeing the training tracks was the pain point.

## Scoring

All numbers live in `config/scoring.toml` (hot-reloaded).

- **Zones** sit in the track file, positioned in section coordinates (anchor index + fraction), so they stay with their corner when anchors move.
  - **Clip point** (inside edge): scored on the **front bumper**'s closest approach while drifting. Pays out once per lap.
    - With `clip_angle_scaled` (on in `config/scoring.toml`, off in older files) the points are also × the angle points' shape at that closest approach, so a clip taken under 20° pays nothing. Why: run9's judged drivers lifted and straightened up at the clips and still got full clip points.
    - Driving forwards out of a clip's window without it paying is a `ClipMissed` event (too wide, not drifting, under 20°, or no combo). Episodes count hits and misses, plus the drift angle and throttle each time the car draws level with a clip.
  - **Outer zone** (outside edge): scored on the **rear bumper**'s proximity to the edge, per second, while drifting through it.
- **Drifting:** angle ≥ 12°, speed ≥ 25 km/h, moving forwards along the track. The drift ends after 0.3 s below 8° (a clean exit). Over 100° is a spin.
- **Points per second while drifting**, × a speed multiplier (0.5× at 30 km/h → 1.5× at 100 km/h):
  - **Angle:** ramps up from 20° (`credit_from_deg`; drifts still start at 12°), full at 40–65°, tapers off towards a spin.
  - **Line:** clips + outer zones.
  - **Style:**
    - transitions (linking direction changes) are a bonus, but only if the side before was held ≥ 1.5 s; a quicker switch is a flick and earns nothing. The bonus is × the angle points' shape averaged over the side just left (`transition_scaled`), so a side held under 20° isn't a transition at all
    - wall taps are a bonus: rear bumper into a hard wall at 0.3–3 m/s; at most one per 2 s and 2 per combo, so they can't be farmed
    - bobbles (angle dips sharply then recovers) are a penalty
- **Combo:** points pile up × a multiplier.
  - The multiplier gets +0.1× per zone, transition or tap, capped at 3×.
  - It's banked on a clean exit, unless the drift lasted under 1 s (`min_bank_secs`): then only its penalties count.
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
| Steer | tanh (-1..1) | `[episode] steering`: `rate` (run9 on) = how fast to turn the wheel, full output going lock to lock in `full_lock_secs`, added up by the driver so the request can't jitter; `position` (run8 and older) = target angle as a fraction of full lock. Either way the road wheels follow at the rack's rate |
| Throttle | sigmoid (0..1) | engine power |
| Brake | sigmoid (0..1) | foot brake, front-biased. Separate from throttle so left-foot braking is possible |
| Handbrake | sigmoid (0..1) | rear brake strength; used to start or extend a drift |
| Clutch | sigmoid (0..1) | clutch pedal (1 = fully disengaged). A clutch kick = pedal in on throttle, revs rise, sharp release → wheelspin |

## Training

- **Genome:** the network's weights as a flat list.
- **Generation loop:**
  1. Evaluate every genome on the same K tracks: fresh procedural tracks each generation (`tracks.per_generation`), plus any saved tracks listed in `tracks.fixed`, plus `tracks.pool_per_generation` drawn from `tracks.pool`. Fixed and pool tracks are left out of the test set.
  2. Aggregate fitness across the tracks.
  3. Elites survive unchanged.
  4. The rest are bred via tournament selection, crossover and mutation.
- **Islands:** several sub-populations, each with its own network shape and activation functions. Breeding happens within an island only. Periodically the worst island is replaced with a copy of the best.
- **Crossover:** neuron-level (a child takes each hidden neuron's whole incoming weight set from one parent).
- **Mutation:** Gaussian noise on a random subset of weights, plus a rare large reset. σ shrinks over time or self-adapts per genome.
- **Fitness (shaped from gen 0):** driving reward × (1 + drift_bonus × drift quality).
  - Driving reward: progress (capped after stage 1) plus time alive.
  - Drift quality is *flow* (below).
  - Stages: drive → slide → judged. The banked judged score (Line/Angle/Style) counts in the judged stage.
  - Advancing (`advance` per stage): the saved driver's metric, averaged over the stage's last `generations` gens (20 by default), must reach `at_least`. The window starts empty on `--resume`. Why: until run9 gen ~1890 it needed `generations` passes in a row, but the value swings ±0.06 per gen with the tracks drawn, so run9 sat at a ~0.36 mean around gen 1700–1830 (passing ~55% of gens) and never got past a 7-gen streak.
  - Shaping stages soften the scoring: drift from 5°, flat speed multiplier, spins don't end the run, no bobble penalty.
- **Flow (drift quality, `[flow]` in scoring.toml):** rewards long sweeping drifts without penalising anything.
  - Each second drifting earns the angle points' shape (full at 40–65°, tapering towards a spin) × a hold credit.
  - Hold credit: 0 → 1 over the first 1 s on one side, then up to 1.5 by 3 s.
  - A link carries 75% of the hold into the next side (up to `link_cap_secs` = 3 s, so the bonus crosses over). A quicker flick, an end or a spin starts it again.
  - Linking is looser than judging: a side held `link_min_hold_secs` (0.75 s) links, even though judging needs 1.5 s for a transition. Files without the keys keep the old rule (judged transitions only, ramp only).
  - Quality = flow-seconds ÷ the run's time limit (`max_secs`). 1.0 = a full-angle drift, each side held past the ramp, for the whole run. A spin on the last corner costs ~8%, one at 10 s ~83%. Older files without the limit fall back to ÷ time lasted.
  - Metrics: `drift_hold` / `top_hold` (seconds per side) and the console's `side` column. The viewer HUD shows the live `flow x` credit.
  - Why: the slide stage learned to wiggle. Quick left/right flicks paid the same raw angle-time as long arcs, and were safer.
  - History: until run8, quality was ÷ the time the run lasted, so a car that spun out at 10 s kept a high quality. Linking came in at run9 gen ~1160: its cars flicked into corner 1 then straightened up instead of linking corner 2, because a sub-1.5 s side reset flow just like gripping. Quality numbers from before each switch aren't comparable.
- **Clutch penalty (`clutch` per stage):** a press shorter than 0.4 s is a kick and is free. Holding the clutch any longer counts, each second weighted by drift angle (0.25 with no angle, rising to 1 at 45°). The run's reward and both Pareto objectives are scaled by 1 − strength × (weighted hold time ÷ run time), never below 0. Strength 2 in every stage. Logged as `clutch_hold` / `clutch_hold_angle`.
- **Smoothness penalty (`smoothness` per stage, `sim/src/smoothness.rs`):** penalises sawing a control while drifting.
  - A saw: changing direction by ≥ 0.2 throttle / 0.3 steer within 0.3 s of its last change of direction (`[episode] smoothness`). Single swings (a lift, a counter-steer catch, a transition) are free.
  - Scales the drift and judged objectives (and their part of fitness) by 1 − throttle weight × throttle saws/s − steer weight × steer saws/s, never below 0.
  - Weights: slide 0.08 / 0.01, judged 0.15 / 0.02. Steering is cheap on purpose: the car has no self-steer, so catching a slide takes real steering.
  - Logged as `best_/mean_throttle_saws`, `best_/mean_steer_saws` and the console's `saws/s` columns.
  - Why: run7's gen-3290 best sawed the throttle ~0.8–2.5 and the steering ~7–16 times a second while drifting.
- **Clip rate (`clips = { target }` per stage, judged in the defaults at 0.6):** the judged points (in fitness and the judged objective) are × min(1, clip hit rate ÷ target), where the hit rate is the share of the clips driven past that paid. A run past no clips keeps everything. Logged as `best_/mean_clip_rate`, `best_clip_angle`, `best_clip_throttle` and the console's `clip` column (a resumed run keeps its CSV's old columns, so these only appear in new runs).
  - Why: run9's judged stage stalled from gen ~2600 hitting roughly 10–25% of the clips, while line points were already over half its judged score.
- **Selection:** Pareto ranking over (driving, drift quality, judged score) inside each island, so slidey crashers survive alongside grip drivers. The judged score (line/angle/style weighted, less penalty) only became an objective after run7: until then the judged stage's weights only fed `fitness` (saved best, logs), so judged points never chose who bred. Logged as `best_judged`.
- **Stage change:** σ resets and 30% of each island is reseeded with heavily mutated copies of its best.
- **Starting priors:** calm start (straight, some throttle, brake off, handbrake/clutch light).
- **Early termination:** a crash, spin, or no progress for a few seconds ends the episode.
- **Diversity:** small tournaments, a few random immigrants per generation; diversity is tracked in behaviour space (lap share, drift share, angle, handbrake, clutch) as well as weight space.
- **Polish phase:** an optional `polish` block on a curriculum stage: smaller σ (reset on entry, no reseed), no immigrants, more generated tracks per generation. Every genome, elites included, is already re-evaluated on fresh tracks each generation.
- **Human picks (`[review]`):** see *Human review*.
- **Saved best (`[best]`):** the top few genomes by fitness also drive some extra tracks (pool tracks not drawn this generation, then generated); the best over all its runs is saved as best.genome, judges curriculum advance and runs the test set, so a lucky draw can't replace a steadier driver.
- **Monitoring:** see *Network view & metrics*.

## Visual style

JDM night: dark asphalt lit by sodium street lamps, neon accents. All viewer-only; the sim and training never see it.

- **Light as additive sprites (`render/light.rs`):** one additive-blend material and three procedural textures (radial glow, headlight cone, vignette). No render targets, so the window's 4× MSAA, video export and the review grid's per-tile viewports work unchanged. Everything is culled to the camera's view.
- **Two passes per world view:**
  - on the road, before the cars: lamp pools, zone glow, headlight beams (so a pack's beams light the road, not each other's paint; the watch screen also dims the pack's beams to 30%)
  - over the cars: tail lights (flaring on the brakes), spark glow
  - then a screen-space vignette (not on review tiles)
- **Street lamps (`render/lamps.rs`):** tracks carry no lights, so they're placed once per track: one on the outside of each corner's apex, then about every 35 m on alternating sides, behind the wall or on the verge fence, never where the edge is open (crossings), at least 18 m apart.
- **Neon:** outer zones and clips glow (stronger while being scored), sparks glow, and the combo glows brighter as its multiplier climbs.
- **Palette (`render/theme.rs`):** asphalt, verge and ground a step darker than before so the pools carry the scene. Light strengths are named constants in `light.rs`, `lamps.rs`, `car.rs`, `track.rs` and `score.rs`.
- `E` (effects) turns the lighting off with the smoke, to compare.
- *(Later)* Katakana accents beside the English labels (English stays the main text): needs a bundled subset of a JP font (Noto Sans CJK JP, OFL), made with fonttools.
- *(Later)* Coloured tyre smoke (per island on watch / review).
- *(Later, from Driver prep)* A camera fly-through at the start of a run, and corners popping up in the HUD.

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

### Video export

- **What:** an MP4 of a replay, under 10 MB, to share. Follow camera with the HUD (speed/angle, score, track info, pedals); no status line, help, timeline, picker, telemetry or network view.
- **How:** the replay screen plays from the start at exactly one recorded frame per video frame (30 fps, so the video doesn't depend on how fast the machine draws) and pipes each frame to the system `ffmpeg` (no new crates): H.264, 720p (width follows the window), CRF 26, with a peak bitrate of 9 MB over the video's length so long runs still fit. The last frame holds 1.5 s.
- **Window:** it captures the window as drawn, so it waits for the window size to settle first (a tiling WM resizes new windows) and stops if the window is resized mid-export. A REC line shows in the window, not in the video; Esc cancels and deletes the partial file.
- **Where:** `R` writes `runs/<run>/videos/<replay>.mp4`; the launcher's "Export video…" (a run's menu) does a replay or the best driver on a saved track; `--video` from the command line.
- **Genome on any track:** driven by the trainer's own episode code with the run's config (same as training), saved as a replay first so the video can be made again.

## Live multi-car viewer

- **What:** `--watch <run>`: the run's top genomes drive one track together, live, from the same rolling start. Ghosts (no car-to-car contact); each runs exactly as in training.
- **Which:** the trainer writes `runs/<run>/top.genomes` every generation (atomically): the top `watch.per_island` (5) of each island by fitness, plus the `watch.drifters` (10) best by drift quality not already in. The file carries the stage's actual rules (episode, scoring, car), so cars run under the rules they were scored on. A newer file is picked up when the current heat ends.
- **Look:** cars coloured by island (validated palette), the followed car highlighted. Full effects (smoke, sparks, damage) on the followed car; every car leaves skid marks. Finished cars stay where they stopped, faded.
- **Controls:** click a car or `F` to follow it; leaderboard panel (rank, island, score, angle, status; click a row; `Tab` expands it); `D` driver info; `M` map; `V` network and `I` inputs for the followed car; `R` restart the heat; `L` / `N` change track (default: the run's fixed track).
- **Expanded leaderboard (`Tab`):** a wide table: place, island, fitness rank and network, fitness, drift quality, age (gens since born), live points, progress, angle or end. Click a column heading to sort by it (default: live points).
- **Driver info (`D`):** a panel for the followed car: who it is (rank, island, network, genome id, fitness, drift quality); lineage (born, age, sigma, parents and children, clickable when racing in the heat); its training runs this generation (objectives driving / flow / judged, mean line / angle / style / penalty, laps, progress, how runs ended); and this heat live (points split, progress, drifting %, above-20° %, biggest angle, spins, zones, clips, transitions).
- **Data:** each `top.genomes` entry also carries `born`, `parents`, `sigma`, `objectives` and a `training` summary. Files written before that still load; age, lineage and training show as unknown until the run is resumed.

## Human review

- **When:** whenever you like. Training never pauses; picks reach the trainer at the end of its current generation. If the trainer isn't running, they wait for the next resume.
- **Where:** the review grid (`--review <run>`, `U` on the watch screen or while following a run, or the launcher's run menu → "Review drivers"; `screens/review/`).
  - **Who:** 12 tiles, 4 across and 3 down (25 was too many to take in): the 8 fittest, then a row of the 4 best drifters from `top.genomes`, re-run live from the same rolling start exactly as in training (the watch screen's `Racer`), one follow camera per tile. Replays aren't used: they hold only the saved best and no genome ids.
  - **Tiles** show the car, its skid marks, rank, network, island colour, live points, and its angle or how it ended.
  - **Controls:** click a tile for a full view (dash, smoke, driver info), Esc back to the grid; `P` / right-click picks the car; Enter or the "Send" button sends the picks; Backspace clears them; `R` restarts the heat; `L` / `N` change track; Space pauses; `[` `]` speed; wheel zooms.
  - A newer `top.genomes` waits for the next heat, and for unsent picks to be sent or cleared.
  - Tiles mark PICKED, SENT (waiting for the trainer) and BOOSTED (applied); the status line says which generation applied the latest send.
- **Channel (`agent::picks`):** each send is one file in `runs/<run>/picks/inbox/`, in the top.genomes format, so it carries the genome (it may be bred out before the trainer reads it). The trainer moves each file to `picks/applied/g<gen>_<file>` (the record of what was boosted when) or `picks/rejected/`.
- **What a pick does (`[review]` in training.toml, `train/src/picks.rs`):**
  - It goes to its own island, or another with the same network shape (a migration may have replaced its island). No match: skipped, with a log line.
  - **Protected** for `protect_gens` (50) generations: carried unchanged after the elites, even if bred out already, and migration never overwrites its island. Saved in checkpoints.
  - **Flood:** `flood_fraction` (30%) of its island is replaced straight away by mutated children of it (its own sigma; lineage kept, so driver info shows them). Several picks in one island share the flood.
  - Logged in train.log: `== pick #id (island i): protected to gen M, K children`.
- **Determinism:** picks are outside input. Resuming from a checkpoint taken before a pick doesn't re-apply it (its file has already moved to `applied/`).
- **Dropped from the first design:** the forced pause every N gens with a timeout, ✓ good / ✗ bad marks, and a fading fitness bonus for descendants.
- *(Later)* A learned style critic: picks + run stats fitted to a small model that becomes the automatic Style score.

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
  - right-click to hide a node's edges (marked with a ring), so a loud input doesn't crowd out the rest; the strongest-X% cut and edge thickness then rank only what's shown. A "show all" chip clears them
- **Outputs** show a value bar and a ~5 s sparkline each.
- **Neuron health:** a rolling 5 s window marks **dead** neurons (never change) and **saturated** ones (pinned at the activation's limits), with counts per layer.
- Driver info for the followed car in the multi-car view: see *Live multi-car viewer*. The review grid's full view shows it too. *(Later)* the same from a replay.

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
- **Logged as:** one row per generation in `runs/<run>/metrics.csv`. *(Later)* human picks marked on the charts, read from `runs/<run>/picks/applied/` (a resumed run keeps its old CSV columns, so a column wouldn't show up in older runs).

**Run dashboard (`C` / `--charts`, `viewer/src/dashboard/`):** the followed run's `metrics.csv`, updating live as the trainer writes. It covers the screen and takes the keys and mouse while open.
- **Pages** (`1`–`4` or click a tab; `pages.rs`). Columns a run never logged are left out.
  - **Overview:** fitness (+ test tracks), drift quality, time above 20°, how runs end, pedal use, behaviour diversity
  - **Scoring:** line / angle / style / penalty for the saved driver and the population, judged objective, top drifter's score, drift angle, seconds per side
  - **Driving:** progress, laps (+ test tracks), biggest angle per run, sawing per second, spins, zones
  - **Health:** best per island, fitness spread, weight diversity, sigma, seconds per generation, sim speed
- **Headline numbers** under the title (`kpi.rs`): best this stage (fitness is shaped per stage, so only this stage counts), the best's trend over 100 gens, generation time, the saved driver's fitness and rank, and time since the last generation (amber when the trainer looks stalled). Beside them, the saved driver against the stage's gate: its rolling mean as the trainer judges it, how full the window is, and the latest value.
- **Reading charts:** legends show the latest generation, or the hovered one, or the one pinned by a click. The tooltip adds each series' trend and its change. Dashed lines mark stage changes.
- **Trend lines** (`trend.rs`): a trailing rolling mean, about 1/20 of the visible generations wide, never looking ahead (so the live right edge means the same as the rest). `S` cycles trend + raw / trend / raw.
- **`Z`** narrows to the last 500, then 100 generations.

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
  config/                    car, scoring, training, generator (hot-reloaded where useful), default_track
  tracks/                    track library (approved + test tracks; deleted/ holds removed ones)
  runs/                      (gitignored) per run: metrics.csv, tracks.csv, best.genome, top.genomes,
                             checkpoints, config snapshot, replays/, videos/, picks/, train.log
  crates/
    common/src/              math (Vec2, angles), rng (PCG), config loading, paths, procs, toml_lines
    sim/src/
      car/                   params, state, wheel, tyre, suspension, engine (+clutch),
                             drivetrain (diff), steering, ground, step (integration)
      track/                 bezier, centreline, boundary, grid, zones, corners, crossings, stats,
                             generator/ (pieces, turtle, rejection), def (file format)
      world/                 one car on one track: surface lookup, physics, collisions, progress
      sensors/               rays, driver_prep, observation (input layout + normalisation)
      scoring/               drift (detection), flow, combo, params
      controls.rs            the 5 outputs
      controller.rs          Controller trait
      episode.rs             run one car on one track, termination rules
      smoothness.rs          control sawing count
      frame.rs, recording.rs replay state frames + sparse records
    nn/src/                  matrix, activation, network (dense layers), init
    agent/src/               driver (Network → Controller), genome_file, replay_file, top_file, picks (review inbox)
    evo/src/                 genome, island, selection (Pareto), crossover (neuron-level),
                             mutation, fitness (curriculum), diversity, config
    train/src/               main, config, runner (parallel eval), checkpoint, metrics (CSV), tracks_log,
                             picks (placing human picks)
    launcher/src/            `cargo drift` menu: runs (status from runs/ + /proc), tracks, video, launch (build + spawn)
    viewer/src/
      main.rs, app.rs        screen state machine
      input/                 gamepad, keyboard, ai, autopilot
      screens/               play, watch (live multi-car), review (4×3 pick grid), replay, tracks (track screen),
                             library (track files)
      dashboard/             run dashboard: pages, chart, kpi, trend, gate, metrics
      render/                camera, car, track, lamps (street lamps), light (additive glow), ground, hud, score,
                             thumb (map cards), theme (JDM palette)
        effects/             smoke, skidmarks, sparks, damage
      debug/                 telemetry, history, inputs, gamepad and driver prep panels,
                             network_view/ (layout, edges, health, inspect, outputs)
      video.rs               MP4 export via ffmpeg
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
