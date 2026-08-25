# Planning Interfaces (Planned vs Market)

Status: designed (v1 slice)
Pillar: 4 — this IS the pillar
Research: [economy-mechanics](../../research/economy-mechanics.md)
principle 7 (quantities vs parameters), plus the dual-currency,
misreporting, and mudflation lessons.

## v1 slice

Every country runs one of two **economic systems** (derived at init:
Eastern bloc + Yugoslavia = Planned, everyone else = Market). Monthly,
each country's industry produces output — throttled by its regions' power
factors and its coal ratio — and splits it three ways: **consumer goods**
(→ standard of living, which demography already consumes), **investment**
(→ industry growth, minus depreciation), **military** (→ stockpile,
inert until the military systems land). What differs is *who controls
the split and what can go wrong*:

### Planned (sets quantities)

- Player command: `SetPlannedAllocation { consumer, investment,
  military }` (permille, sum 1000) — direct quotas, chunky 5% steps in UI.
- **Misreporting**: the apparatus pads disappointing results. The country
  tracks `actual` and `reported` industry; reported drifts up to +15%
  above actual when growth misses the allocation-implied expectation.
  The planned player's own dashboard shows REPORTED. (Espionage will
  later steal true numbers — including your rival's view of you being
  more accurate than your own.)
- Defaults for AI/unmanaged: 350/450/200 (heavy-industry tilt).

### Market (sets parameters)

- Player command: `SetMarketPolicy { interest_bp, tax_permille,
  procurement }`. Firms decide the split: investment share responds to
  the interest rate, households take the remainder after procurement,
  military share follows the procurement level (Low/Med/High).
- **Inflation**: loose money + procurement above tax capacity builds
  inflation (smoothed), which eats standard of living. Defaults are
  neutral (no punishment for not touching anything).
- Market dashboards show TRUE statistics (honest statistics are the
  system's quiet advantage).
- Defaults: interest 4%, tax 25%, procurement Med (≈ 560/240/200 split).

### Shared consequences

- SoL becomes **dynamic**: `SoL = 5 + consumer-goods per capita (scaled)
  + urban-share bonus − inflation penalty`, replacing the static formula;
  demography's vital rates, urbanization, and education now respond to
  policy. Guns-vs-butter is finally a real dial with demographic teeth.
- Industry growth: `Δ = output × invest_share × CONVERT − depreciation
  (2‰/mo)` — depreciation per research principle 1 (run to stand still).
- Fixed-point integers throughout (industry in centi-points).

## Cadence & placement

Monthly, `TickSet::Economy`, after balances
(demography → balances → production chain). Commands apply in
`TickSet::Commands` with validation: planned commands rejected for
market countries and vice versa; allocations must sum to 1000.

## UI (v1)

`E` toggles an economy panel (right side). Both systems see: industry
(reported vs true per the rules above), last growth, SoL, power factor,
national ratios. The control block differs: quota rows with −/+ buttons
(5% steps) for planned; lever rows (interest ±0.5%, tax ±2.5%,
procurement cycle) for market. Same panel skeleton, different verbs —
the first-hour difference the pillar demands.

## Cut from v1 (deliberate)

- Five-year-plan ceremonies, binding public targets, plan congresses.
- Hard currency / dual currency (needs trade).
- Recessions/business cycles beyond inflation; unemployment.
- Black markets, queues; regional allocation (national only).
- System transitions (locked at scenario start).

## Open questions

- Conversion asymmetry (planned crash-speed vs market compounding):
  v1 uses identical CONVERT; differentiate when production methods land.
- Misreporting discovery: should audits (a command) reset reported→actual
  at a stability cost? Leaning yes, next iteration.
