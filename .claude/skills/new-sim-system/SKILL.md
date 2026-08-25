---
name: new-sim-system
description: Scaffold a new gameplay simulation system in ugs-sim (e.g. tension decay, production, influence drift) following the project's determinism and scheduling conventions. Use when adding any mechanic that changes sim state.
---

# New simulation system

Adds a gameplay system to `crates/ugs-sim` the right way. Follow every step.

## 0. Preconditions

- A design doc for the mechanic exists in `docs/design/systems/` with
  `Status: designed` (or better). If not, stop and run `/design-doc` first —
  do not implement from a `sketch`.

## 1. Decide placement

- **Module**: one file per mechanic under `crates/ugs-sim/src/` (e.g.
  `tension.rs`, `production.rs`). Register in `lib.rs`.
- **TickSet stage**: pick exactly one — Time / Commands / Economy /
  Politics / Military / Resolve. If it needs two stages, it's two systems.
- **Cadence**: hourly (rare — combat/crises only), daily
  (`if !clock.new_day { return; }`), or monthly (`clock.new_month`).
  Never use tick-modulo arithmetic.

## 2. Write the system

- State lives in `Resource`s or `Component`s with `serde` derives (they
  will be save-game state). All numbers that a designer might tune go in a
  `pub mod tuning` block in the same file, or in data files — no magic
  literals inside logic.
- Randomness: fork a dedicated stream once — `rng.fork(b"<mechanic>")` —
  and store it in the system's resource. Never roll from the root `SimRng`
  inside per-entity loops.
- If iterating entities where order affects results, collect and sort by a
  stable key (id) first.
- Register: `app.add_systems(SimTick, my_system.in_set(TickSet::<Stage>))`
  inside `SimPlugin::build`.

## 3. Tests (required, same file)

Use the pattern in `crates/ugs-sim/src/lib.rs` tests: build a headless App
with `SimPlugin`, call `run_ticks`, assert on resources. Minimum set:

1. Behavior test at the mechanic's cadence boundary (day/month rollover).
2. Determinism test: two apps, same seed, run 1000+ ticks, assert relevant
   state bit-identical.
3. One edge case from the design doc's "open questions" or limits section.

## 4. Finish

- `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
- Update the design doc `Status:` to `implemented` and note any deviations
  from the design in the doc.
