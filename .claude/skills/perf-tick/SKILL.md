---
name: perf-tick
description: Measure and diagnose simulation tick performance. Use when adding heavy systems, when speed 5 feels sluggish, or before merging changes that touch per-province/per-country loops.
---

# Tick performance

Budget: at speed 5 the sim runs 168 ticks/sec. Frame budget at 60fps is
~16ms, so **all sim work must average well under 0.1ms/tick** at target
scale (~10k provinces, ~150 countries). Most systems achieve this by being
daily/monthly cadence, not hourly.

## Measure

1. Bench harness: `crates/ugs-sim/benches` (create with criterion if absent:
   `criterion = "0.5"` under `[dev-dependencies]`, bench that builds a
   headless app at realistic scale and times `run_ticks(&mut app, 24 * 30)`).
   Realistic scale means synthetic data at ~10k provinces if the real map
   isn't there yet — generate it in the bench setup, seeded.
2. Report per-tick mean and the cost of one full month.
3. For hot-spot hunting: `cargo flamegraph -p ugs-sim --bench <name>`
   (needs `cargo install flamegraph`; on macOS runs via dtrace, may prompt
   for sudo — ask the user to run it if so).

## Diagnose — usual suspects

- Hourly cadence work that should be daily/monthly (check the `new_day`
  guard exists and is FIRST in the system).
- Per-tick allocation in loops (collect-and-sort allocating fresh Vecs —
  reuse buffers via `Local<Vec<_>>`).
- O(provinces × countries) cross products; precompute per-country province
  lists and maintain them incrementally on ownership change.
- BTreeMap point-lookups in hot loops — fine for determinism, but hot paths
  can carry a sorted Vec + binary search or an index map built once.

## Report

State the numbers plainly: ticks/sec achieved headless, projected speed-5
frame cost, before/after if you changed something. Determinism is never
traded for speed — any optimization must keep the determinism-check suite
green.
