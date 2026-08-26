# Economic agency & regional legibility

Status: designed
Pillar: 4 (the systems-competition pillar becomes playable; asymmetric
verbs over one substrate), feeding 1 (enrichment projects), 2
(project influence payouts), 3 (espionage-filtered foreign dossiers).
Research: [economic-agency](../../research/economic-agency.md)
(7-analyst swarm + synthesis, 2026-08-26), building on the locked
[economy-mechanics](../../research/economy-mechanics.md) architecture.
Siblings: [economy](economy.md), [resources-and-grids](resources-and-grids.md),
[planning-interfaces](planning-interfaces.md).

The economy runs but the player watches it. This slice converts
watching into playing with one loop: **see a place → read its computed
constraint → commit a project → get graded by next month's document.**
Built by thawing state the sim already computes; the military command
layer (player decides what/where, sim executes) is the template.

## Player-facing description

- **The Ledger** (econ panel, REGIONS tab): one sortable row per owned
  region — NAME | POP | IND | PWR% | CONSTRAINT | trend. Planned
  countries see PLAN/REPORTED columns side by side (the dashboard is
  somebody's word); market countries see true figures stamped a
  quarter late ("SURVEY, Q4 1949").
- **The Region Dossier**: click a region row (or a province on the
  map) — a fixed one-page teletype document: header (name, owner, pop
  + trend, character line "COAL / HEAVY INDUSTRY"), POWER block with a
  generation-vs-demand bar, PRODUCTION with deposit pips, active
  project card with progress and "slowed by" cause, and a footer that
  prints the binding-constraint verdict sentence ("OUTPUT LIMITED BY
  POWER: GENERATION COVERS 82% OF DEMAND") beside verb buttons.
  Foreign regions render the same form through espionage penetration
  at 2 significant figures; unknown blocks print "NO COVERAGE".
- **ECONOMY map mode**: regions tinted by constraint kind in three
  severity bands — the triage list as a map.
- **Projects** (econ panel, PROJECTS tab): a construction pool accrues
  from your investment allocation; 2 generic slots + 1 Great Project
  slot. Planned players place industrial expansions, power stations,
  and agricultural mechanization on named regions (wire decree on
  start, ceremony on completion). Market players place only public
  works (power, Great Projects) — industry placement belongs to
  firms: set up to 3 development zones and steer a PUBLISHED
  deterministic allocator; the dossier attributes the response
  ("PRIVATE INVESTMENT THIS MONTH: +0.4 — power surplus, zone
  active"). The offer board lists condition-gated Great Projects; the
  USSR starts with the Volga-Don Canal already at ~60% — an inherited
  portfolio before the first move.
- **Heartbeat**: the econ OVERVIEW leads with last month's 3–6
  severity-ranked lines, each naming a region and a cause. All
  economic figures are monthly, date-stamped, still between
  boundaries — nothing wiggles per tick.

## State (all serialized, digested unless marked derived)

- `RegionalIndustry` (new resource):
  `by_region: BTreeMap<RegionId, u64>` centi-points, seeded once from
  the existing urban-share split of country industry. Authoritative
  distribution; `IndustryState.actual_centi` becomes the maintained
  cache of the country sum (dashboards, misreporting, intel keep
  working unchanged).
- `Construction` (new resource):
  `pool: BTreeMap<CountryTag, u64>` (centi), `next_id: u32`,
  `projects: BTreeMap<ProjectId, Project>` where
  `Project { country, region, kind, progress_centi, cost_centi,
  started_tick, slowed_by: Option<ConstraintKind> }`,
  `zones: BTreeMap<CountryTag, BTreeSet<RegionId>>` (≤3),
  `attribution: BTreeMap<RegionId, u64>` (last month's private inflow,
  derived-but-digested), `log: Vec<(u64, String)>` ring (60, derived,
  undigested), `offers_taken: BTreeSet<String>`.
- `ProjectKind { IndustrialExpansion, PowerStation, AgriMechanization,
  Great(String /*catalog id*/) }`.
- `RegionSnapshots` (new resource, DERIVED — rebuilt monthly, excluded
  from digest like `BattleView`; sim decisions must never read it):
  per region `{ pop, pop_trend_permille, industry_centi,
  reported_centi, power: PowerStatus, constraint: ConstraintKind,
  severity: Severity, private_last_centi }` plus a per-country ranked
  `wire: Vec<String>` of last month's lines.
- `ConstraintKind { Power, Materials, Labor, Healthy }`,
  `Severity { Healthy, Strained, Critical }`.
- ugs-data: `GreatProjectDef { id, name, country, province (site, by
  name), offered: OfferCondition { AtStart { progress_permille },
  Date((i32,u8,u8)), PowerDeficit, GrainShortfall }, cost_centi,
  min_months, payload: ProjectPayload { Power(u64), Industry(u64),
  AgriYield(u32), Enrichment(u32), Influence(i32) }, blurb }` loaded
  from `assets/data/scenario/1950/projects.ron`, validated (country
  and province must exist).

## Cadence & formulas (constants in `construction::tuning`)

All in `TickSet::Economy`, a new `update_construction` +
`update_snapshots` chained after `planning::update_production`.

**Regional production (the thaw, in `update_production`).** Country
output = Σ over owned regions of
`region_centi × region_power_factor / 1000`, × the national materials
ratio and held-fraction as today. Growth: investment gains and
depreciation apply to `RegionalIndustry` (planned: proportional to
existing distribution; market: via the allocator below), then
`actual_centi` is set to the sum. Regional industry raises its own
grid demand through the existing demand formula — more factories in
Kuzbass brown Kuzbass out.

**Pool accrual (monthly).** `pool += invest_out × DIRECTED_PERMILLE
(500) / 1000`; the other half converts to industry growth exactly as
today. Guardrail: pool above `POOL_CAP_MONTHS (6) × typical project
cost` auto-converts to proportional industry growth — an AI or
passive player loses nothing to the new system (zero new AI code).

**Projects.** Costs `PROJECT_COST_CENTI`: IndustrialExpansion 600,
PowerStation 900, AgriMechanization 500; Great from catalog. Monthly
intake per project = `pool_draw × host_power_factor / 1000 ×
materials_ratio / 1000` — the cannot-buy-itself gate; `slowed_by`
records the argmin factor when intake < nominal, printed on the card
and the wire. `pool_draw = min(pool / active_projects,
INTAKE_MAX_CENTI (120))`. Completion applies the step-change:
IndustrialExpansion `+EXPANSION_CENTI (400)` region industry;
PowerStation `+STATION_CAPACITY` into the region's generation
capacity (sized `= region demand × 400‰` at start, min floor);
AgriMechanization `+MECH_YIELD_PERMILLE (50)` to the country's
agriculture yield (regional agriculture is post-slice);
Great per payload (Enrichment feeds the nuclear facility level;
Influence is logged until the influence pillar lands — deviation).
Site modifiers shown at start and applied to `cost_centi`: deposit in
region −10%, power surplus −5% (stations exempt), idle-labor bonus
−5%, power-deficit +10% for non-station projects.

**Market allocator (monthly).** Market countries route
`invest gains × PRIVATE_PERMILLE (700)` through a published integer
score per owned region:
`score = power_factor/10 + urban_permille/10 + ZONE_BONUS (40 if
zoned) − tax_permille/20`, distributed largest-remainder,
written into `RegionalIndustry` and `attribution`. Planned countries
skip the allocator (proportional growth). Development zones:
`SetDevelopmentZone { country, region, on }`, cap
`ZONE_CAP (3)`, planned countries rejected (wrong-interface pattern).

**Commands.** `StartProject { country, region, kind }` — validated:
region owned (1950 owner) and majority-held, slot free (generic
`GENERIC_SLOTS (2)`, Great 1), kind legal for the economic system
(market: PowerStation and Great only), pool ≥ 10% of cost, Great id
must be currently offered and unbuilt. `CancelProject { country, id }`
refunds `CANCEL_REFUND_PERMILLE (300)` of remaining progress cost.

**Snapshots & constraint (monthly, last in Economy).** Per region:
`power_permille` (grid factor), `materials_permille` (national),
`labor_permille = clamp(urban_pop × LABOR_SCALE / industry_centi)`;
`ConstraintKind` = argmin, `Healthy` if argmin ≥ 900; severity
`Critical < 700 ≤ Strained < 900`. Reported column: the country's
existing reported/actual national ratio applied proportionally (no new
drift state — the fast-follow adds per-region falsification + audits).
Wire lines from snapshot deltas crossing thresholds, ranked by
severity, capped `WIRE_LINES (6)`; also pushed to `Construction.log`.

## Interactions

Reads/writes `Economies` (invest split, stock untouched), `RegionalPower`
(capacity written by stations, factors read), `Demographics` (labor,
pop trends), `Agriculture` (mechanization yield), `NuclearPrograms`
(Enrichment payload → enrichment_level), `Military` (held-fraction
validation; war can sever a project's region — project pauses with
`slowed_by = Materials`), espionage `Intel` (foreign dossier fidelity,
UI-side), `FiredEvents` (Great Project completion ceremony notice).

## AI note

None needed: AI countries never start projects in v1 — the pool-cap
auto-conversion returns their money to today's growth path, so their
behavior is bit-compatible with the current economy. (AI project
selection is a post-slice; the offer conditions and score formula are
the hooks.)

## Edge cases

- Region loses majority control mid-project: intake 0, card prints
  "SUSPENDED — REGION CONTESTED"; liberation resumes it.
- Pool at 0 with active projects: intake 0, `slowed_by = Materials`.
- Two projects, one pool: split evenly (pool_draw), deterministic.
- Zone on a lost region: dropped at monthly pass with a wire line.
- Cancel Great Project: allowed, id returns to the offer board,
  `offers_taken` keeps it from double-completion only.
- Snapshot never read by sim systems (derived, like BattleView — the
  settlement review's lesson is codified here from day one).
- Country with zero regions (city-states): ledger shows one row,
  everything national.

## Deliberately not modeled (v1 cuts)

Per-region falsification drift, the AUDIT verb, and diegetic tells
(first fast-follow); contested foreign bids / Aswan auctions (second);
commodity priority flags; the market Contract Book; hard-currency
funding; sovnarkhoz reform; sim-drafted proposal engine (the briefing
recommendation line is UI-derived only); regional agriculture and
labor drafts; auto-pause briefing popup (the OVERVIEW leads with the
briefing instead — popups stay reserved for events).

## Open questions (leanings)

- Should completed stations depreciate like industry? Leaning yes,
  folded into the existing depreciation term, post-slice.
- Great Project failure events (Virgin Lands dust bowl): v1 ships the
  payload + a dated reversal event in events.ron rather than a new
  stochastic system — revisit with the shock engine.
