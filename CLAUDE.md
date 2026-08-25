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
