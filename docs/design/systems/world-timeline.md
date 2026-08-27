# World timeline & the living-world engine

Status: implemented (engine + 1950-1970 corpus)
Pillars: all four — the timeline is what makes the pillars matter
everywhere, not just in Korea. Alignment flips are pillar 2 made real;
decolonization is the influence battleground; the systemic arcs
(Stalin's death, Sputnik, the split) drive pillars 1 and 3.
Research: rests on the existing corpus
([sovereignty-1950](../../research/sovereignty-1950.md) for the map,
[war-termination](../../research/war-termination.md) for outcome
objects); content sourced per event from Wikipedia by regional
research agents, cited in data-file comments.

Eleven events cover 1950–52 in Korea. This pass makes the whole world
move to 1970: every region gets its arc, playable countries get real
decisions, and new states are born on schedule. The engine principle,
decided explicitly: **content is configuration.** Anything a 1950–70
event needs to express must be a data-file trigger or effect, never a
hardcoded special case — the grammar below is the contract between
the sim and the content swarm.

## The event grammar (configuration surface)

**Files**: `assets/data/scenario/1950/events/*.ron` — one file per
region/arc, merged at load in sorted filename order (the legacy
`events.ron` still loads first). Event ids must be globally unique
(validated).

**Triggers** (existing + new):
- `Date((y, m, d, h))` — on/after.
- `WarDaysElapsed { a, b, days }`.
- `ProvincesLost { owner, count }`.
- NEW `TensionAbove { tenths }` / `TensionBelow { tenths }` — the
  world reacts to the temperature (a thaw event needs calm; a
  crackdown needs crisis).
- NEW `EventFired { id, days_after }` — chains: Geneva follows Dien
  Bien Phu.
- NEW `OptionChosen { id, option, days_after }` — decisions matter:
  the consequence arc of *choosing* differently. This is the
  alt-history hinge.
- `chance_permille` still gates any trigger daily once its condition
  holds (spread, not scripts).

**Effects** (existing + new):
- Existing: `AdjustTension`, `DeclareWar` (truce/deterrence-checked),
  `SetPosture`, `TransferProvinces`, `SpawnForces`, `GrantStock`,
  `SetWarAim`, `GrantLegitimacy`, nuclear-program effects.
- NEW `SetAlignment { country, alignment }` — Cuba 1961, Egypt after
  Suez, Albania to the Chinese pole (v1: the three-valued alignment;
  a finer influence scale is the influence pillar's job later).
- NEW `AdjustStability { country, delta }` — coups, uprisings,
  assassinations shake or steady governments (stability already
  exists as CountryDef data; this makes it dynamic).
- NEW `GrantIndustry { country, centi }` — Marshall-scale aid and
  war damage, landing in the recipient's regions proportionally.
- NEW `Independence { country, from, provinces }` — see below.

## Dynamic world state

Static `CountryDef` fields become *baselines* with runtime overrides
on the `Military` resource (pragmatically the "world state" resource;
rename to consider post-slice): `alignments: BTreeMap<CountryTag,
Alignment>` and `stability: BTreeMap<CountryTag, u8>`, serde-default,
digest-folded. Every alignment read (bloc basing, patrons, same-side,
red lines, the market allocator's bloc checks, map political tinting
for blocs) goes through `Military::alignment_of(data, tag)`;
stability reads likewise. **Rule: no system may read
`CountryDef.alignment/stability` directly** — enforced by review, so
one SetAlignment effect flips every downstream behavior at once.

## New-country formation (decolonization)

Region-granularity, exploiting the map invariant that regions never
cross 1950 borders and colonial clusters have their own regions:

`Independence { country, from, provinces }` — `provinces` is a short
list of names (one per intended region suffices) owned by `from` in
1950. Execution: resolve names → regions → for each region, reassign
`EconomyStatic.region_owner` to the new tag (industry, power,
snapshots, and per-country sums all follow automatically because they
key off region ownership); every province in those regions enters the
occupation overlay as held-and-recognized by the new tag (map color,
military ownership); the new country's manpower pool seeds at the
standard 1.5% of the transferred population; a wire notice announces
the birth. The parent's economy shrinks by exactly the ceded regions
(production's "own provinces" check moves to region-ownership too, so
no double penalty). New-country `CountryDef`s (tag, name, color,
capital, alignment, stability, industry≈0) ship in
`countries/independence.ron` — dormant until their event fires
(zero regions owned = inert; the nation-select screen keeps listing
only 1950 sovereigns as playable starts).

Armies, aid, and posture at birth compose from existing effects in
the same event (`SpawnForces`, `GrantStock`, `SetAlignment`).

## Content guidelines (the swarm's brief)

- **Coverage**: every region an arc; every *playable* 1950 sovereign
  ideally touched by at least one event by 1970; the majors get 4–8
  real decisions each. ~45 new states 1951–70 with historical dates.
- **Decisions must trade off**: an option that is strictly better is
  a bug. Price choices in the game's real currencies (tension,
  legitimacy, stability, stock, alignment).
- **Chains over lone events**: use `EventFired`/`OptionChosen` so
  choosing differently *goes somewhere* (refusing Suez, backing
  Mossadegh, splitting with Moscow earlier).
- **The sim already does some history**: don't script what emerges
  (arms buildups, brinkmanship crises, settlements). Script the
  *political* facts the sim can't derive: leadership deaths,
  independence dates, treaty organizations, coups.
- **Teletype voice**: ALL-CAPS wire copy, period-honest, specific.
- **Source every event** in a RON comment (Wikipedia article title
  suffices); dates must be real dates.
- **Bind names to the map**: agents grep `provinces/world.ron` and
  `countries/generated.ron` for exact province names and tags before
  writing them. Integration re-validates via the loader.

## Cadence & determinism

Unchanged: events evaluate in `TickSet::Politics`; chance rolls are
daily from the `b"events"` stream; fired/resolved gain tick stamps
(`fired_ticks`, `resolved_ticks`) to power the chain triggers —
serde-default maps, digest-folded.

## Edge cases

- Independence of already-transferred regions: re-resolution is a
  no-op (region already owned by target).
- Independence while the parent is at war / region occupied by a
  third party: provinces under enemy occupation stay occupied — the
  new state is born into a claim, not a possession.
- SetAlignment on a superpower: legal but content must not (review).
- Chain triggers referencing unknown ids: load-time validation error.
- Two events firing the same tick with conflicting effects: applied
  in file/definition order, deterministic.

## Deliberately not modeled (this pass)

Leaders as sim objects (deaths are events + stability deltas; the
nations_meta dossier stays 1950); elections; a finer alignment scale
(influence pillar); migrating `events.ron` content; UN membership;
civil wars as sub-national actors (coups are stability + alignment
changes); economic union objects (EEC is events + GrantIndustry).

## Implementation notes

- Shipped corpus: 189 events across nine regional files plus the
  legacy Korea set; 37 new-state CountryDefs in
  `countries/independence.ron`; every map name grep-verified by the
  content agents and re-validated by the loader (which caught three
  cross-file duplicate ids and one Chad capital rename at
  integration).
- `tools/timeline/integrate.py` is the swarm-output integration path:
  binds capital names to province ids, lays out event files, reports
  problems. Re-run it for future content passes.
- The acid test `the_world_turns_to_1970` runs twenty hands-off years
  (~15s): the spine fires, a dozen-plus new states own regions, Cuba
  is Eastern-bloc, and 1970 is not a world war.
- CI regime floors and economic-system seeding deliberately stay on
  the 1950 baseline (regime character, not current alignment).

## Post-review notes

- Independence became PER-PROVINCE (id-bound) after review: colonial
  regions are super-regions holding many future states, so
  whole-region transfer let the first-born swallow its neighbors
  (Guinea annexing French West Africa). Province lists are generated
  from Natural Earth adm0 attribution with centroid disambiguation
  for generic colonial names; load-time validation enforces ownership
  and cross-event disjointness; region ownership follows the majority
  holder. Some border provinces stay colonial where names could not
  be disambiguated — historically messy edges, refined later.
- Wartime mobilization keys off the CURRENT holder, so new states
  mobilize their own people and parents no longer draw on ceded
  ground.
- Chain-trigger state (fired/resolved stamps) is digest-folded.
- Known emergent quirk, accepted: a very late-ending Suez war can
  leave an ISR-EGY truce alive in June 1967, degrading the Six-Day
  War to a "TRUCE HOLDS" notice — plausible alt-history, not a bug.

## Open questions (leanings)

- Should `Military` be renamed `WorldState` once it owns alignments?
  Leaning yes, in a mechanical refactor commit, post-content.
- Post-1970 timeline: same grammar, new files — nothing here caps it.
