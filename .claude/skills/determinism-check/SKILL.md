---
name: determinism-check
description: Audit the simulation for determinism violations — divergent runs, ordering hazards, forbidden entropy sources. Use after sim changes, when a desync/replay bug is suspected, or periodically as a health check.
---

# Determinism check

The sim must be bit-identical given the same seed + commands. This skill
audits that property two ways.

## 1. Static audit (grep pass)

Search `crates/ugs-sim`, `crates/ugs-map`, `crates/ugs-data` for forbidden
patterns and inspect each hit:

- `std::time`, `Instant`, `SystemTime` — no wall clock in the sim.
- `thread_rng|from_entropy|rand::random` — all randomness via `SimRng`.
- `HashMap|HashSet` — allowed only where iteration order provably cannot
  affect outcomes; prefer `BTreeMap`. Flag every iterated one.
- `f32|f64` — allowed for now (single-platform), but flag any accumulation
  across ticks fed from the presentation layer, and any `sin/cos/pow` on
  results that gate discrete outcomes (platform-varying libm).
- `.iter()` over Bevy `Query` where effects depend on order without a
  sort-by-stable-key first.
- `ugs-sim/Cargo.toml` gaining any dependency beyond bevy_ecs/bevy_app/
  serde/ugs-data/ugs-map — the headless boundary must hold.

## 2. Dynamic audit (divergence test)

Ensure a test exists (add/extend `crates/ugs-sim/tests/determinism.rs` if
missing) that:

1. Builds two headless apps with `SimPlugin`, identical seeds.
2. Runs both for a long horizon (≥ 90 in-game days = 2160 ticks) —
   interleaving the two apps' ticks, which catches cross-app static/global
   state leaks that running them sequentially would miss.
3. Serializes all sim resources/state each in-game day and asserts equality;
   on mismatch, report the first divergent day and which resource diverged.
4. Also runs a third app with a different seed and asserts it DOES diverge
   (guards against the test comparing constants).

Run it with `cargo test -p ugs-sim`. As new state resources are added, they
must be included in the comparison — keep a single `snapshot()` helper that
new systems are required to register into.

## 3. Report

Summarize: violations found (file:line), risk level (breaks determinism now
vs. hazard later), and fix applied or recommended. Zero findings is a valid
result — say so plainly.
