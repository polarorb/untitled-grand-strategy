# Untitled Grand Strategy

A Cold War grand strategy game in Rust + Bevy. Real-time-with-pause, HoI4-scale
province map, starting January 1st, 1950 — six months before the Korean War.
Inspired by Hearts of Iron 4 but its own game: the four design pillars are
**escalation & nuclear brinkmanship**, **ideology & influence warfare**,
**intelligence & covert ops**, and **economic systems competition**.
See `docs/design/vision.md` before designing any new mechanic.

## Workspace layout

| Crate | Purpose | May depend on |
|---|---|---|
| `crates/ugs-data` | Static data types + RON scenario loaders | serde, ron only |
| `crates/ugs-map` | Immutable province graph, adjacency, pathfinding | ugs-data |
| `crates/ugs-sim` | Deterministic simulation core (headless) | bevy_ecs, bevy_app, ugs-data, ugs-map |
| `crates/ugs-app` | Bevy presentation: window, rendering, UI, input | bevy (full), all of the above |

**The dependency arrow never points from sim to app.** `ugs-sim` must never
gain a dependency on full `bevy`, rendering, assets, audio, or windowing.
If the sim needs to tell the UI something, it emits state the UI reads —
never the other way around mid-tick.

Game content lives in `assets/data/scenario/<name>/` as RON files, validated
at load (`ScenarioData::load`). The world map (4,594 provinces / 226
countries / polygon geometry) is GENERATED — see `tools/mapgen` and the
`scenario-data` skill; never hand-edit `world.ron`, `generated.ron`, or
`assets/map/world.geo.ron`. Design docs live in `docs/design/`; every
non-trivial mechanic gets a doc there before implementation.

## Determinism (non-negotiable)

The sim must produce bit-identical results from the same seed + same commands.
This is what makes saves small (seed + command log), replays possible,
multiplayer lockstep feasible, and bugs reproducible.

- One tick = one in-game hour. The only way time advances is the `SimTick`
  schedule via `ugs_sim::run_ticks`. Systems run in `TickSet` stages
  (Time → Commands → Economy → Politics → Military → Resolve); order across
  systems goes through those sets, never ad-hoc `.after()` chains.
- All randomness comes from the `SimRng` resource or streams forked from it
  with stable labels (`rng.fork(b"combat")`). Never `thread_rng`, never
  hashing, never time.
- No `std::time`, no wall clock, no floats accumulated from frame deltas
  inside `ugs-sim`. The presentation layer owns real time.
- Never iterate `HashMap`/`HashSet` where order affects outcomes. Use
  `BTreeMap`/`BTreeVec`-style ordering or sort first. Query iteration order in
  Bevy is not guaranteed: when a system's effects depend on entity order,
  collect and sort by a stable key first.
- Daily/weekly/monthly cadence work hangs off `SimClock::new_day` /
  `new_month` flags, not tick-count modulo arithmetic.

## Commands

```sh
cargo test --workspace          # all tests, including headless sim ticks
cargo run -p ugs-app            # launch the game
cargo run -p ugs-app --features fast-compile   # dynamic linking, faster builds
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Bevy is pinned at the 0.19 line. Its API moves fast and training data is
usually stale — when a Bevy API doesn't compile, check the 0.19 docs/migration
guide rather than guessing older idioms.

## Research before implementation

For any non-trivial mechanic or content decision, research FIRST, then
design, then implement:

- **Genre/design questions** → agent swarm (Workflow tool with schema-
  validated output; the user has opted into swarms for research). Distill
  results into `docs/research/<topic>.md` (concise readup + raw JSON
  alongside) before touching the design doc. Precedent:
  `docs/research/economy-mechanics.md` (8-analyst swarm, ten convergent
  principles that now govern the economy architecture).
- **Historical facts/data** → the `historian` agent; sourced, with dates.
- **Bevy APIs** → the `bevy-scout` agent (training data is always behind
  Bevy's release pace; verify against the pinned version's vendored source).
- Design docs cite their research; implementation follows the
  `new-sim-system` skill.

## Asset generation

Two pipelines, both documented in the `asset-gen` skill:

- **Sourced assets** (flags, portraits, any historical imagery):
  Wikimedia Commons via `tools/nationgen/fetch_assets.py` — resolves
  exact `File:` titles through the API, rate-limit aware, records
  licenses. EVERY shipped asset gets a line in `assets/CREDITS.md`.
- **Generated assets** (backgrounds, missing portraits, UI art):
  nano-banana 2 (`gemini-3-pro-image`) via
  `tools/nationgen/generate_art.py` (needs `GEMINI_API_KEY`, present in
  the user's shell env; never print it). Mark all AI-generated assets as
  such in CREDITS.md. Prefer sourced over generated when a real asset
  exists.
- Fonts: SIL OFL only, fetched as static TTFs via the Google Fonts CSS
  API; current faces are Oswald (display), Jost (UI), Courier Prime
  (dossier text) in `assets/fonts/`.

## GitHub & the Pages site

Remote: `github.com/polarorb/untitled-grand-strategy` (public). GitHub
Pages serves the `docs/` folder of main at
https://polarorb.github.io/untitled-grand-strategy/ — so **docs commits
are site deploys**. Push after committing; keep these current as part of
finishing any feature:

- `docs/index.md` — landing page; update the feature list/screenshots
  when something player-visible lands (screenshots go in `docs/media/`,
  captured via the `UGS_SHOT` env var below).
- `docs/devlog.md` — the development blog, newest entries first. Add an
  entry for every substantial work session: what was built, the
  interesting technical/design decisions, dev-perspective (not marketing).
- Design docs and research notes under `docs/` are part of the site;
  write them knowing they're public.

## Dev environment shortcuts

- `UGS_SCREEN=select|game` — boot directly into a screen.
- `UGS_NATION=TAG` — play as a nation (with `UGS_SCREEN=game`).
- `UGS_MAPMODE=terrain` — boot in terrain map mode.
- `UGS_SHOT=path.png` — the game screenshots its own window after
  `UGS_SHOT_FRAMES` frames (default 120; raise it when the shot needs sim
  time to pass first). Use for visual verification; never screencapture
  the desktop.
- `UGS_SPEED=1..5` — boot unpaused at that speed (e.g. to get past the
  first month boundary so monthly systems have output).

## Conventions

- New sim mechanics: write the design doc (or use the `design-doc` skill),
  then implement with tests that run real ticks headlessly (see
  `crates/ugs-sim/src/lib.rs` tests for the pattern). Use the `new-sim-system`
  skill for scaffolding.
- Gameplay numbers (base costs, modifiers, thresholds) belong in data files
  or clearly-marked `tuning` modules — not scattered as magic literals.
- Newtype all ids (`ProvinceId`, `CountryTag`). Never pass raw `u32`s.
- Historical content must be sourced; use the `historian` agent for research
  and record sources in the data file's comments.
- Test cadence boundaries: month rollover, leap years (1952 is one), and the
  campaign start date 1950-01-01.
