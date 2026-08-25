# Demography

Status: designed
Pillar: substrate for all four (the universal denominator — see
[economy research](../../research/economy-mechanics.md), principle 3)

## What it is

Per-province population in three continuous cohorts — **rural**, **urban
worker**, **educated** — seeded from the real 1950 census data (HYDE total
+ urban counts), evolving monthly through births, deaths, urbanization,
and education. Never discrete pop objects. This is the input layer for
labor, conscription, consumption, and unrest; in v1 it runs and displays
but nothing consumes it yet.

## Player-facing

Province card shows the cohort split; trends are visible over years.
The world grows: baby boom in the West, population explosion in the
decolonizing world, and the whole strategic arc 1950→1990 emerging from
one curve.

## State

`Demographics` resource: `BTreeMap<ProvinceId, Cohorts>` where
`Cohorts { rural: u64, urban: u64, educated: u64 }` in persons (integer;
deterministic). Initialized on the first tick from scenario data:
`urban = urban_k`, `rural = population_k − urban_k`, `educated = urban ×
edu_share₀(SoL)`.

`LivingStandards` resource: per-country SoL 0–100, v1 static, computed at
init: `SoL = clamp(5 + 60·(industry / pop_M) + 40·urban_share, 5, 80)`.
(1950 result: USA ≈ 68, GBR ≈ 53, SOV ≈ 44, IND ≈ 14 — right ordering.)
Later: SoL becomes dynamic from consumer-goods consumption.

## Cadence & formulas

Monthly (`new_month`), `TickSet::Economy`. Annual rates in ppm, applied
monthly as `pop × rate / 12 / 1_000_000` (u128 intermediate, floor).

- Births (all cohorts): `clamp(48 − 0.38·SoL, 16, 48)` per 1000/yr.
  Newborns join their cohort's pool (children are not modeled separately
  in v1 — cut, see below).
- Deaths: `clamp(34 − 0.42·SoL, 9, 35)` per 1000/yr.
  (USA → 9.4 vs historical 9.6; IND → 28 vs 28; GBR → 11.7 vs 11.8.)
- Urbanization: `clamp(0.2 + 0.035·SoL, 0.2, 2.5)` % of rural moves to
  urban per year.
- Education: `0.3 + 0.015·SoL` % of urban converts to educated per year.

Constants in `demography::tuning`. Calibration note: the Western baby
boom makes 1950 birth rates anomalously high vs SoL; per-country vital
overrides in data are the planned fix once results are compared against
UN historical tables (post-v1).

## Interactions

- Reads: scenario data (initial pops), country stats (SoL inputs).
- Writes: nothing yet. Future consumers: labor supply (industry),
  conscription pools (military), food demand (agriculture), emigration
  (influence), scientist pool (nuclear program).
- UI reads cohorts for the province card.

## AI note

None needed in v1 (no decisions). Later: AI reads cohort trends for
development and conscription policy.

## Edges & cuts (v1)

- No age structure (cohort-age bands come with conscription).
- No migration between provinces or emigration across borders (comes
  with the influence pillar).
- SoL static; no feedback from economy yet.
- Provinces with zero population stay zero (uninhabited islands).
- Determinism: all-integer math, iteration over `BTreeMap` only.
