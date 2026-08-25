---
name: scenario-data
description: Add or edit scenario content — countries, provinces, and future data types — in assets/data/, with historical sourcing and load-time validation. Use when expanding the 1950 scenario or adding map regions.
---

# Scenario data

Game content lives in `assets/data/scenario/<name>/` as RON, loaded and
validated by `ugs-data::ScenarioData::load`. Types are in
`crates/ugs-data/src/lib.rs`.

## Workflow

1. **Source first.** For any historical numbers (population, industry,
   leaders, dates, borders), delegate to the `historian` agent and get
   figures with sources. Record the source in a `//` comment at the top of
   the data file. Never invent plausible-sounding numbers silently — if
   estimating, comment `// estimate:` with the reasoning.
2. **Respect id ranges.** Provinces: reserve blocks per region and note them
   here as they're claimed. Currently claimed:
   - 1–9: great-power capitals (placeholder)
   - 10–99: Korean peninsula
   Claim the next free block for a new region and update this list.
3. **Country tags** are 3 uppercase letters, unique, stable forever (saves
   will reference them). Prefer obvious ones (FRA, GBR, JAP, IND).
4. **Adjacency is symmetric** — the loader rejects one-way links. When
   adding a province touching an existing region, update both files.
5. **Validate**: `cargo test -p ugs-data` runs the load-and-validate test
   over the real assets. It must pass before you're done. If you added a new
   data type, extend `ScenarioData::validate` with its cross-reference
   checks and add a loader test.

## Editing types

When gameplay needs a new field: add it to the struct in `ugs-data`,
decide a sensible default (`#[serde(default)]` keeps old files loading —
prefer this during development), update ALL existing data files if the field
is meaningful per-entity, and extend validation for its legal range.

## 1950 accuracy bar

The Jan 1, 1950 start should be defensibly accurate: correct governments,
alignments (Yugoslavia split from Moscow in '48; PRC proclaimed Oct '49;
NATO exists since Apr '49; no Warsaw Pact until '55 — model it as informal
Eastern bloc), and no anachronisms (no West German army yet, no H-bomb).
When in doubt, ask the historian agent.
