# Time & Map

Status: designed (time), sketch (map)

## Time

- One sim tick = one in-game hour; the `SimTick` schedule is the only way
  time advances (see `crates/ugs-sim`).
- Real-time with pause; speeds 1–5 = 1 / 4 / 12 / 48 / 168 game-hours per
  real second (speed 5 ≈ a week per second).
- Subsystems run on cadences via `SimClock::new_day` / `new_month`:
  hourly (combat, crises), daily (movement, production, influence drift),
  monthly (budgets, plans, elections checks, escalation decay).
- Campaign start: 1950-01-01 00:00. End-of-timeline target: 1991 eventually;
  vertical slice covers 1950–1953.

## Map

Target: HoI4-scale provinces (~10k eventually) grouped into **states**
(economic/political unit) grouped into **strategic regions** (theater unit).
Influence/espionage/economy operate on countries and states; the military
game operates on provinces.

Build order (deliberately incremental):

1. **Now**: hand-authored RON provinces for Korea + capitals (done —
   `assets/data/scenario/1950/`). Enough to exercise loaders, graph,
   pathfinding, and first combat.
2. **Next**: generate provinces from real-world geodata (Natural Earth +
   population rasters) via an offline `tools/mapgen` crate that outputs RON;
   hand-tune Korea first, then Europe.
3. **Later**: full globe, naval sea zones, strait/canal rules, terrain from
   elevation data.

Rendering: start with flat polygon map (Bevy 2D), political + terrain +
alignment map modes. Alignment mode is the signature view — the world as
shifting red/blue/neutral, the scoreboard you stare at.

## Determinism notes

Adjacency is symmetric and validated at load. Pathfinding tie-breaks by
province id. Any future map data generation runs offline, never at runtime.
