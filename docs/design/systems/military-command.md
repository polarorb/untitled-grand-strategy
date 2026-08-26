# Military command & force generation

Status: designed
Pillar: 1 (escalation — mobilization, commitment, and ROE are
brinkmanship signals) and 4 (the economy finally pays for the army).
Research: [military-mechanics](../../research/military-mechanics.md) §3
(theaters & directives), §6 (manpower as capital stock), §7 (readiness
states as mobilization signals). Parent architecture:
[military](military.md). Stress-tested by design review 2026-08-26;
all findings incorporated.

This is the v1 spec for the command layer that
[military.md](military.md) sketches: it gives the player the three
levers currently missing — **how many divisions exist and where they're
based** (force generation), **which are active vs reserve** (readiness),
and **where the army fights and how it spreads out** (theaters). It
replaces the blob-march ("all formations BFS toward the nearest enemy")
with theater-scoped frontline distribution.

## Player-facing description

The war room gains a **FORCES** tab and a **THEATERS** tab.

- **Forces**: divisions listed **grouped by theater**, with batch rows
  ("all → Reserve", "assign selected to theater X") — no obligatory
  per-row bookkeeping at 30+ divisions. Per division: name, archetype,
  strength/cohesion, readiness, training bar. Buttons: RAISE (pick
  archetype and a repeat count ≤ 5; home = currently selected
  province), DISBAND, readiness toggle (Active ⇄ Reserve), theater
  assignment. Raising shows the military-stock and manpower price up
  front.
- **Theaters**: create/rename/delete theaters; while a theater is
  selected for editing, clicking provinces on the map paints them
  in/out. Per theater: posture (**Defend / Probe / Offensive**), up to
  3 objective provinces (clicked on the map), an **Echelon** slider
  (share of the theater's divisions held back from the front), and an
  ROE list of countries whose soil this theater may never enter ("may
  not cross the Yalu" is a checkbox on the China line).
- On the map, theaters tint their provinces; each formation marker
  shows its theater's color chip. Mobilization and wartime force
  expansion print wire notices — on the enemy's wire too (both are
  public signals).

The player never moves a division directly. They decide what exists,
where it's based, which theater owns it, and the theater's directive;
province-level positioning is executed by the theater logic.

**Where you may operate.** Theater painting and division homes are
legal on provinces owned or held by you **or any co-belligerent**
(a country you share a war with). Raising a division with home on
co-belligerent soil IS the v1 overseas-deployment abstraction: the
division trains up in place there, the 90–150 train days standing in
for shipping and staging. This is what lets the US build up in Korea —
there is no naval transport in v1. Painting a province into one of
your theaters removes it from any other theater of yours (exclusive).

## State

All on the `Military` resource unless noted; everything serializable,
everything new folded into the digest (existing pre-theater saves load
with `training = 1000`, `readiness = Active`, `theater = None`).

- `Formation` gains:
  - `readiness: Readiness { Active, Reserve, Mobilizing { days_left: u8 } }`
  - `theater: Option<TheaterId>`
  - `training: u16` permille (1000 = fully trained; scripted/OOB spawns
    start at 1000)
  - (`quality` already exists on `Formation` and already scales combat;
    this system moves it — arrears decay, recovery — around its
    archetype baseline.)
- `TheaterId(u32)` newtype; `Military.next_theater_id: u32`.
- `Military.theaters: BTreeMap<TheaterId, Theater>` where
  `Theater { owner: CountryTag, name: String, provinces: BTreeSet<ProvinceId>,
  posture: TheaterPosture { Defend, Probe, Offensive },
  objectives: Vec<ProvinceId> /* ≤3 */, echelon_permille: u16,
  forbidden: BTreeSet<CountryTag>, auto: bool }`.
- `Military.upkeep_accrued_centi: BTreeMap<CountryTag, u64>` (daily
  accrual, settled monthly), `Military.upkeep_arrears: BTreeMap<CountryTag, u16>`.
- Per formation, presentation-invisible: `slot: Option<ProvinceId>`
  (current front assignment) and `retarget_cooldown: u8`.
- Economy: `IndustryState.military_stock` (already accumulating) is now
  spent — no new economy state.

## Commands

New `SimCommand` variants. Validation: formations/theaters must belong
to the issuing country; provinces for painting and homes must satisfy
the co-belligerent rule above; objectives are rejected if they lie in a
`forbidden` country.

`RaiseFormation { country, archetype, home, count /* 1..=5 */ }`,
`DisbandFormation { country, id }`,
`SetReadiness { country, id, active: bool }`,
`CreateTheater { country, name }`,
`PaintTheater { country, id, province, add: bool }`,
`DeleteTheater { country, id }`,
`AssignTheater { country, formation, theater: Option<TheaterId> }`,
`SetTheaterPosture { country, id, posture }`,
`SetTheaterObjectives { country, id, objectives }`,
`SetTheaterEchelon { country, id, permille }`,
`SetTheaterRoe { country, id, tag, forbidden: bool }`.

The existing country-level `SetPosture { Hold, Advance }` survives as
the AI/default layer (below); the war-panel posture button now edits
the auto-theater's posture instead.

## Cadence & formulas

Scale anchors: `military_stock` accrues ≈ `industry × mil_share`
points per month (US at 10% ≈ 10/month; PRK at 30% ≈ 2.4/month).
Tension is internal tenths, 0–1000, displayed ÷10. All constants live
in `military::tuning`.

**Raising (daily, TickSet::Military).** `RaiseFormation` debits
`RAISE_STOCK_COST` (Infantry 3, Motorized 5, Armor 8) per division
immediately and spawns at `home` with `strength = 100`,
`training = 0`, Active, assigned to the owner's theater containing (or
nearest to) `home`, else `None`. It fills toward 1000 strength through
the *existing* reinforcement pipeline (15 str/day from the manpower
pool; a division is 10,000 men from real population) and trains
`1000/TRAIN_DAYS` per day (`TRAIN_DAYS`: 90 inf / 120 mot / 150
armor). Combat attack/defense scale by `500 + training/2` permille —
green troops fight at half weight, but *can* fight immediately;
committing half-trained divisions is a real Korea decision (Task Force
Smith). Insufficient stock ⇒ no-op logged to the wire ("PROCUREMENT
SHORTFALL").

Force expansion is a public escalation signal: each raise **at peace**,
and each raise **at war beyond the country's peace floor** (below),
adds `RAISE_TENSION = 3` tenths and prints on both wires. Only money
limiting wartime buildup would let the rich side snowball past the
limited-war thesis; commitment itself must carry a price until the
full intervention-level system lands.

**Upkeep (accrued daily, settled monthly).** Daily accrual in
centi-stock at the division's *current* state — Active: 1/30 of
`UPKEEP_CENTI_STOCK` (20 inf / 30 mot / 50 armor per month); Reserve
and Mobilizing: 20% of that. Divisions based or fighting outside their
owner's contiguous home territory accrue at
`OVERSEAS_UPKEEP_MULT = 3×` (20 US divisions in Korea cost
12 stock/month — the allocation-vs-butter squeeze is the point).
Daily accrual makes readiness-flapping around the month boundary
pointless. At month end the accrued total (rounded up) is debited,
**stock first, down to zero**; any shortfall sets
`upkeep_arrears += 1` and that month every division's `quality` decays
`20 × shortfall_fraction` permille (floor 500). While
`arrears ≥ 1`: reinforcement halts and every division loses
`ARREARS_MELT = 30` strength/month — an unpaid army *melts*, it never
plateaus into a free force. A fully-paid month clears arrears; quality
recovers 10/month toward the archetype baseline.

**Readiness (daily).** Active → Reserve is immediate (cohesion drops
to 300). Reserve → Active enters `Mobilizing { days_left:
MOBILIZE_DAYS = 21 }`; until `days_left = 0` the division is treated
as Reserve for upkeep, slots, movement, and defense weight, then
becomes Active at cohesion 300. Reserve/Mobilizing divisions cannot
move, take no front slots, defend at 70% weight where they sit, and
reinforce at half rate only after all Active divisions. Each
activation at peace adds `MOBILIZATION_TENSION = 3` tenths (wire
notice both sides); at war, activation is free — the war *is* the
signal.

**Theater assignment (daily, replaces the global nearest-enemy
march).** Per theater in `TheaterId` order, formations in
`FormationId` order — an assignment-*preserving* controller, not a
daily re-deal:

1. *Front set.* Defend: theater provinces adjacent to a province held
   by a country the owner is at war with. Probe: those, plus enemy
   provinces adjacent to them. Offensive: Defend's set, plus the enemy
   provinces on the BFS path from it toward each objective — offensive
   slots ARE enemy soil; that is how an offensive advances. Empty
   front set (no war touches the theater) ⇒ formations disperse
   across theater provinces one per province nearest-first, wrapping
   round-robin when formations outnumber provinces — peacetime
   garrisons.
2. *Slot weights*, additive:
   `1 + hostile_adjacent_count + 3·on_objective_path + 2·enemy_formation_adjacent`
   (each term a flag or count; objective-path is boolean — overlapping
   objective paths don't stack).
3. *Quotas.* Deal the theater's committed divisions (all non-echelon,
   non-reserve) across front provinces proportionally to weight,
   largest remainder, stable order. The top `echelon_permille` share
   by `FormationId` desc — deliberately the newest, greenest
   divisions — instead hold at the theater province nearest the
   front-set centroid that is not itself in the front set.
4. *Stability.* A formation whose `slot` is still in the front set and
   whose slot is over quota by ≤ 1 keeps it. Only provinces with
   deficit ≥ 2 pull from provinces with surplus ≥ 2, moving the
   nearest eligible formation (FormationId asc tiebreak), and a
   formation that retargets sets `retarget_cooldown = 3` days. No
   front-shuffle, no apportionment flicker.
5. *Execution.* Each formation takes one BFS hop toward its slot with
   the existing `find_advance_step` machinery and cooldowns, plus two
   constraints: never enter a province owned or held by a `forbidden`
   country (ROE — violation is impossible by construction in v1), and
   posture gates enemy soil: Defend never enters it, Probe only
   front-set slots, Offensive anything on the way to its slot.

**Concentration soft-cap (hourly, in combat).** Until real frontage
lands, each side's total contribution in a battle scales by
`min(1, (3 + hostile_adjacent_edges) / committed_divisions)` — the
fourth-plus co-located division adds nothing beyond what local
geometry supports. This closes the "paint a one-province theater,
re-create the blob on purpose" exploit; smooth and additive, cleanly
replaced by geometric frontage later.

Formations with `theater: None` walk home and sit (the standing-around
army you have to organize — deliberate). Occupation flips, combat,
retreat, armistice: unchanged.

**Disband.** Returns `strength × 10 × 80%` men to the manpower pool;
training is lost. Disband-and-re-raise elsewhere is the de facto
rebase, priced in training time — acceptable v1 clunk.

**Objectives lifecycle.** An objective is auto-cleared (with a wire
note) when captured by the theater's owner side or when its holder
leaves the war.

## AI & defaults

Every country at war with no player-made theater gets one
**auto-theater** (`auto: true`, created on war start, deleted at
peace): all own + occupied provinces, posture mapped from the country
`Posture` (Hold → Defend, Advance → Offensive), objectives = enemy
capital, echelon 0, ROE = every country it is not at war with. The
player's country gets the same auto-theater, fully editable — a player
who never opens the Theaters tab gets today's behavior, minus the
blob.

AI readiness: on war start, and monthly while at war, an AI country
issues `SetReadiness(active=true)` on its reserves, staged at ≤ 5
activations/day — a readable 21+-day mobilization ramp the enemy's
wire and intel see. AI force generation (monthly, at war): raise in a
**4:1 infantry:armor pattern** (armor only if affordable) whenever
`military_stock ≥ cost + 2 × monthly_upkeep` and fielded men
< `manpower_pool / 3`. At peace it keeps `PEACE_FLOOR_DIVS = 2`
active (majors — industry ≥ 50 — keep `industry / 20`), rest in
Reserve. Scripted `SpawnForces` events still work and spawn Active,
auto-theater assigned, training 1000.

## Interactions

- **Economy** (`planning.rs`): `military_stock` becomes real; the econ
  panel's stock readout gains a burn rate. Accrual formula unchanged.
- **Tension** (`tension.rs`): peacetime mobilization and force
  expansion add tenths as above; nothing reads theaters.
- **Events** (`events.ron` / `events.rs`): `SpawnForces` unchanged.
  New event effect **`GrantStock { country, amount }`** — patron aid.
  The `chinese-intervention` chain must use it (SOV/PRC carrying
  PRK's war): PRK at 2.4 stock/month cannot sustain 10 active
  divisions under upkeep, *by design* — without patron aid its army
  melts by mid-1951, which is the historical shape. KOR should sit
  marginal (arrears-adjacent, decaying quality in summer 1950 is
  correct flavor), carried by US `GrantStock` in the intervention
  events. The `us-intervention` chain stays as the scripted floor;
  the player can now build past it — the 4-division ceiling is gone.
- **Intel** (`intel.rs`): enemy counts already display as estimate
  bands; reserve/mobilizing status shows one band coarser. Deeper
  coupling (spotting postures) is post-slice.
- **Armistice/settle** (`settle_wars`): unchanged; player theaters
  persist through peace, auto-theaters don't.

## Edge cases

- Raise with 0 stock, disband last division, paint-empty theater: all
  legal no-ops or inert.
- Theater fully enemy-occupied: front set computed from *current
  holder*, so everything is front; formations fight to re-enter, else
  hold.
- Formation's theater deleted ⇒ `theater = None` (walks home).
- Home province hostile: raising into it is rejected; reinforcement
  already requires resting on friendly soil.
- Objective in a country that joins `forbidden` later: cleared with a
  wire note at the next daily pass.
- Caps: ≤ 8 theaters/country, ≤ 3 objectives, ≤ 200
  formations/country (sanity bound, logged if hit).
- Digest: theaters, readiness, training, arrears, accrued upkeep all
  folded in; determinism tests extended.

## Deliberately not modeled (v1 cuts)

- No per-unit move orders — ever (research rule zero).
- No supply/logistics; no geometric frontage (the soft-cap stands in);
  no encirclement changes.
- No air or naval forces; no transport — co-belligerent basing is the
  deployment abstraction; ports and amphibious planning come with
  logistics.
- No equipment designer coupling — archetype costs are flat until the
  designer lands (sequencing item 2 in military.md).
- No conscription-law policy layer (manpower stays 1.5% + wartime
  0.2%/month).
- No accidental ROE violations or ROE-as-priced-escalation — v1 ROE is
  a hard movement constraint; the priced version arrives with the
  escalation-ceiling work, which will also replace `RAISE_TENSION`
  with real intervention levels.
- No multi-national combined theaters (US and ROK fight in parallel
  theaters; coalition command is post-slice).

## Open questions (post-slice leanings)

- Should Offensive posture drain cohesion theater-wide
  (tempo/culmination governor)? Leaning yes, with the fatigue system.
- Reserve pools as intel objects with estimate quality — leaning yes,
  with the intel-domain coupling.
