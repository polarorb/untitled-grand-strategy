# Ideology & Influence Warfare

Status: implemented (v1 slice; deviations noted below)
Pillar: 2 (primary); feeds 1 (coups priced in tension, regional gates
by band), 3 (coups and election pushes are espionage operations;
political penetration gets its first consumer), 4 (aid draws from the
construction pool — a factory not built at home).
Research: [influence](../../research/influence.md) (7-analyst swarm,
two skeptics, synthesis, 2026-09-02).
Siblings: [espionage](espionage.md), [war-termination](war-termination.md),
[world-timeline](world-timeline.md), [newspaper](newspaper.md),
[tension](tension.md).

North star: **the map is painted by alignment, not occupation.** Every
country carries one signed position between the poles; the blocs move
it with capped standing programs and priced covert operations; the
timeline, settlements and occupation write into the same number; and
the player is scored on what the map says at 1955, 1960 and 1965.

## Player-facing description

- **INFLUENCE map mode**: a diverging hue (Western blue ↔ Eastern red,
  olive for the non-aligned band) whose saturation steps with depth;
  hatched where a lock holds (treaty, satellite, neutrality);
  outlined where a contested window is open; pips on the sourced
  battleground set. Foreign positions below LIMITED political coverage
  render desaturated: you are looking at an estimate.
- **The politics panel** (`P`, `UGS_PANEL=influence`): the default tab
  is a ~12-row BATTLEGROUNDS triage list — COUNTRY | POSITION band
  with the estimate bracket | DEPTH | TREND and its cause word | NEXT
  valve | OURS | THEIRS (an estimate or "?") — with filter chips MY
  BLOC, CONTESTS, ALL. A COMMITMENTS tab shows programs n/cap and the
  active-op slot. Clicking a row opens a one-page **political
  dossier**: header, the axis strip with its AS-OF stamp, a signed
  PRESSURES ledger of at most six named terms ("US AID +10", "NAM
  PULL -5", "DECAY TOWARD BASELINE +5"), a NEXT block (election date,
  window close, lock expiry), a verdict footer, and verb buttons whose
  disabled state prints the reason ("LOCKED: WARSAW PACT", "NO SLOT",
  "NETWORK TOO WEAK", "CLOSED AT CRISIS: EUROPE").
- **Verbs**, two price shapes. *Standing programs* occupy a slot and
  persist with zero upkeep: AID (draws centi from your construction
  pool every month, so it competes with your own projects) and
  PRESENCE (radio and missions, near-free, halved in closed regimes).
  Dropping an aid program is itself the decision: WITHDRAW shoves the
  target the other way and adds tension (Aswan, July 1956). *Active
  operations* occupy the op slot and run through your intelligence
  network: ELECTION PUSH (offered when a battleground democracy's
  sourced election is within six months; a bounded nudge on the
  result, three points as history says) and SPONSOR COUP (gated on
  visible facts — stability below 50, an army not equipped by the
  rival, no lock, the tension band below the region's gate; three
  months of preparation with clean abort; a four-rung outcome ladder
  in estimative words with one legend). Even a clean coup pays tension
  and legitimacy. A blown one moves the target *against* you.
- **The mirror**: your own bloc is a filter chip in the same ledger.
  Satellites hold while locked; the existing uprising events gain
  priced options — CRUSH re-locks, costs legitimacy and tension, and
  raises the next rising's severity; TOLERATE unlocks and opens the
  rival's aid verb on that country.
- **Decolonization**: every Independence opens a 24-month contested
  window — rates doubled, hysteresis suspended, the paper's COLONIAL
  QUESTION table lists open windows with months remaining and believed
  lean. A program may be placed on the parent territory from the date
  independence was publicly announced; the newborn inherits its lean.
- **The score**: regional PRESENCE / DOMINATION / CONTROL standings
  over the battleground set, recomputed monthly, printed in the paper
  as INFLUENCE STANDINGS, and frozen at 1955-01-01, 1960-01-01 and
  1965-01-01 as the era checkpoints the victory rule reads. A
  non-aligned battleground counts as DENIED to both.
- **Heartbeat**: at most six wire lines a month, each with a named
  cause, ranked by weight × |delta| × battleground; pull cards only
  for election windows, contested windows, coup eligibility and
  uprisings. No renewals, no upkeep, no nagging.

## State

`Influence` resource (`crates/ugs-sim/src/influence.rs`), all
`BTreeMap<CountryTag, _>` unless noted:

| Field | Type | Meaning |
|---|---|---|
| `position` | `i16` -1000..=1000 | +West, −East; the OccupationZone unit |
| `baseline` | `i16` | the seed; program-bought lean decays toward it, never past |
| `lock` | `Lock { until_tick: u64, label: String }` | treaty / satellite / neutrality; `u64::MAX` = open-ended |
| `contested_until` | `u64` | independence window close tick |
| `last_band_change` | `u64` | reversal within 12 months runs at half rate |
| `army_patron` | `Pole` | who equips the army — the coup gate |
| `closed` | `BTreeSet<CountryTag>` | regime openness; halves PRESENCE, sets the CI floor, no elections |
| `crushed_count` | `u8` | never decays; raises the next rising's severity |
| `slots`, `op_slots` | `u8` | program / op caps per sponsor (seeded; unlock events add) |
| `presence_unlocked` | `BTreeSet<CountryTag>` | USSR at start; USA via Campaign of Truth |
| `programs` | `BTreeMap<(sponsor, target), Program { kind, tier, started_tick, delivered_centi }>` | standing programs |
| `ops` | `BTreeMap<(sponsor, target), InfluenceOp { kind, resolve_tick, election_idx }>` | prepared operations |
| `elections_fired` | `BTreeSet<u16>` | index into the calendar |
| `standings` | `BTreeMap<Region, RegionStanding { west, east, denied, verdict }>` | monthly |
| `checkpoints` | `Vec<(GameDate, standings)>` | frozen at the era dates; digest-folded |
| `chequebook` | `BTreeMap<CountryTag, Vec<(CountryTag, ProgramKind)>>` | last month's AI attributions |
| `wire` | `Vec<(u64, String)>` | ring of 60, excluded from the digest |
| `seeded` | `bool` | first-tick seeding done |

Alignment stays the derived enum: `project()` sets
`Military.alignments[tag]` from the band — immediately after shoves,
coups, locks, clauses and independence, and at the month rollover for
program flows, elections and zone releases — so all eleven
`alignment_of` consumers (basing, patrons, red lines, the nuclear
ultimatum, the masthead) are untouched. **No code path writes
`Military.alignments` except `project()`.**

Scenario data, hand-authored in `assets/data/scenario/1950/influence.ron`
(loaded into `ScenarioData.influence`, optional, validated):
- `seeds`: per tag — position, lock (label + until year), army patron,
  open/closed, sourced stability (overrides the flat 60 at seed time),
  slots, op slots, and for the 37 newborns the baseline pole and the
  date independence was announced.
- `battlegrounds`: tag, region, weight 1-3; `thresholds`: per region the
  Presence / Domination / Control counts.
- `elections`: date, tag, what was at stake, the historical result
  line (printed as the wire text).
Every row carries a source comment (`historian` agent, 2026-09-02).

## Cadence and stage

`update_influence` runs in `TickSet::Politics` after `update_events`
and before `intel::update_intel` (so a coup resolved this tick reads
this month's network and the paper reads this month's band). Hourly:
seed before the first command flush; resolve due operations; fire
elections whose calendar date has arrived (the first tick of the day). Monthly (`SimClock::new_month`): program
flows, decay toward baseline, the NAM pull, hysteresis and band
projection, window closes, standings, the AI allocator, the checkpoint
freeze, the wire.

## Formulas (`influence::tuning`)

Bands and depth. Enter a bloc band at |pos| ≥ `BAND_ENTER = 300`, leave
at |pos| < `BAND_LEAVE = 150` (evaluated at month rollover; inside a
contested window both thresholds are 300). Display depth: LEANING
< 300, ALIGNED < 700, TREATY/SATELLITE ≥ 700 or locked. A country that
changed band in the last `REVERSAL_MONTHS = 12` months moves at half
rate against the change.

Programs. AID tier t draws `AID_CENTI_PER_TIER = 300` × t from the
sponsor's construction pool each month (skipped, with a wire line, if
the pool cannot cover it); lands `AID_ANNOUNCE = 50` toward the sponsor
the month it starts, then `AID_FLOW = 10` × t per month. PRESENCE
lands `PRESENCE_FLOW = 5` × t, halved in closed regimes. Both ×2 for
small states (population < `SMALL_STATE_K = 5000`) and ×2 inside a
contested window; halved while the sponsor's legitimacy is below
`LEGIT_MALUS = -20` (the Budapest bill lands on what the crusher
competes for). A program auto-suspends with a wire line when the
target locks against the sponsor. WITHDRAW of an aid program that has
delivered at least one month shoves `WITHDRAW_SHOVE = 100` away from
the sponsor and applies `WITHDRAW_TENSION = 50` (5.0).

Passive motion. Only two rules, both bounded: if no program is active
on a country, its position moves `DECAY = 5`/month toward `baseline`
(never past it); after `bandung-conference` has fired, an unlocked
country adjacent to a NAM champion (IND, EGY, YUG, IDN, and GHA once
born) moves `NAM_PULL = 5`/month toward 0. There is **no** living-
standard drift and **no** gravity toward zero for baseline positions
(both killed by the skeptics: they repaint the hands-off world).

Elections. A calendar row fires at hour 12 of its date if the country
is open and unlocked-or-locked-toward-its-own-bloc. `swing = roll(-100..=100)` from
`rng.fork(b"elections")` + `ELECTION_PUSH = 60` per push for/against +
`sign(position) × 20` (incumbency). `position += swing`. The band then
changes only if the hysteresis gate clears at the next rollover, so
France and Italy hold Western through a decade of 25% communist votes.
The wire prints the row's result line.

Coups. Gate: `stability_of(target) < COUP_STAB_GATE = 50`; `army_patron
∉ {rival}`; no lock; tension band below the region's gate (Crisis
closes EUROPE; Brink closes EUROPE, ASIA and MIDDLE_EAST; nothing may
force a coup at Brink); network strength ≥ `OP_MIN_STRENGTH` (spent
`OP_STRENGTH_COST` at launch); a free op slot. Resolves `COUP_PREP_DAYS
= 90` after launch. Frontier score `s = (50 − stability) × 10 +
network_tier × 100 + (army_patron == sponsor ? 200 : 0) − band × 50`;
success permille `p = clamp(300 + s, 50, 900)`, printed as a Kent word
(≥ 800 ALMOST CERTAIN, ≥ 600 PROBABLE, ≥ 400 CHANCES ABOUT EVEN,
≥ 200 PROBABLY NOT, else ALMOST CERTAINLY NOT) with the frontier
line ("STAB 38 · ARMY: US-EQUIPPED · NETWORK L2 · TENSION WARY").
Roll from `rng.fork(b"coup")`; exposure from the espionage blown roll
(`OP_BLOWN_BASE + ci/4 + band × 40`). Ladder:

| Rung | Success | Blown | Effect |
|---|---|---|---|
| CLEAN FLIP | yes | no | position = sponsor edge (±350), stability −25, army_patron = sponsor, closed, contested window closes; tension +150 in a battleground else +50; legitimacy −5; DynamicChoice RECOGNISE / STAY QUIET |
| FLIP WITH EVIDENCE | yes | yes | as above, plus deniability −20, legitimacy −10, tension +40 more, COVERT ACTION EXPOSED notice |
| QUIET FIZZLE | no | no | stability −5; nothing else |
| EXPOSED FAILURE | no | yes | position −150 against the sponsor, deniability −20, legitimacy −10, tension +40, notice; the target's `army_patron` hardens to the rival if it was None |

The CIVIL WAR rung is deferred (needs the war machinery).

Standings. Per region: `west` = battlegrounds in the Western band,
`east` likewise, `denied` = the rest; verdict for each pole from the
region's thresholds (CONTROL ≥ control and rival ≤ 0 … PRESENCE ≥ 1).
Non-aligned scores DENIED for both. Checkpoints freeze a copy at the
first tick of 1955, 1960 and 1965 and print THE RECKONING in the paper.

AI allocator. Monthly, for USA and SOV (skipping the player), reading
a month-start snapshot into a delta map applied in tag order.
Candidates: not self, not at war with the sponsor, not locked against
it, |pos| < 700, and (a battleground or an open contested window or an
announced newborn). `score = (weight × 1000 + pop_k / 100) × (1000 −
|pos|) × (contested ? 2 : 1)`; fill free slots with the top candidates
by (score desc, tag asc) — AID if the pool covers a tier-1 draw, else
PRESENCE; drop a program whose target sits ≥ 600 in the sponsor's
favour (job done). No "rival active here" multiplier. AI blocs run no
coups or election pushes in v1; the historical coups stay timeline
events whose `SetAlignment` is now a band-edge shove.

## Event grammar additions

Effects: `ShiftAlignment { country, delta }`; `LockAlignment { country,
months, label }` (months 0 = unlock); `Crush { patron, country }` =
lock 60 months + `crushed_count` += 1 + position to the patron's edge;
`SetArmyPatron { country, patron }`; `GrantInfluenceSlot { country,
ops: bool }`; `UnlockPresence { country }`; `OpenContest { country,
months }`. `SetAlignment` keeps its name and becomes a band-edge shove:
position = ±350 (or 0) only if not already in that band.
Triggers: `AlignmentBand { country, band }` and `StabilityBelow
{ country, value }`, so chains can branch on who won a contest.
`Independence` additionally seeds `position = baseline_pole × 150`,
opens a 24-month window, and inherits any program lean accumulated on
the announced tag.

## Interactions

Reads position/band: `settlement::patrons_of`, `same_side`,
`red_line_triggered`, `military::friendly_soil` (all via
`alignment_of`, unchanged); `intel::ci_permille` (now reads `closed`
instead of static 1950 alignment — the Cuba-after-1961 bug); the paper
masthead; the victory rule (checkpoints).
Writes position: events (shoves, shifts, locks, crush), programs, ops,
elections, `Independence`, settlement clauses — `ClientState` sets the
patron's edge and locks for the truce term; `Neutralization` sets 0 and
locks (neutralized states now exclude foreign basing with no new code)
— and a released occupation zone flows `zone.alignment / 10` into the
country position through the holder's bloc frame.
Consumes: the construction pool (aid), intel networks and the blown
roll (ops), `Settlements.legitimacy` (the one world-opinion currency),
`GlobalTension` (coups, withdrawals, exposure).
Unchanged by design: no trade or market access from alignment (no
trade model exists), crisis flashpoints (still hard-coded; the
cheapest pillar-1 consumer, deferred one step).

## Edge cases

- Position at ±1000: clamped; programs still draw (the sponsor sees
  "NOTHING LEFT TO BUY" and the wire suggests dropping it — the AI
  drops at 600).
- Both blocs run aid on the same target: the flows net; the dossier
  shows both terms. The chequebook line prints the rival's spend
  through political penetration: exact at EXTENSIVE, "ACTIVE" at
  PARTIAL, "?" below.
- A lock expires: no shove; the position simply becomes movable and
  hysteresis applies from the current value.
- A newborn with no seed row: baseline pole None → position 0, window
  opens anyway.
- Player is a minor with 0 slots: the panel is read-only with the
  reason printed ("NO STANDING PROGRAMS: MINOR POWER").
- A coup on a country at war: refused ("AT WAR — USE THE WAR ROOM").
- Elections while occupied: skipped with a wire line.

## AI note

Only the allocator above. Elections resolve for AI democracies from
the calendar; uprising events resolve to the data default (option 0);
AI never launches a coup in v1. This is deliberate and stated: the
rival owns the slow verbs, the timeline owns the historical coups.

## Presentation

`MapMode::Influence` (icon, `M` cycle, `UGS_MAPMODE=influence`).
`influence_ui.rs` panel as above, with the same button-to-command
discipline as the other panels. Paper: INFLUENCE STANDINGS (bloc
totals by states and population at two significant figures, regional
verdict words, top movers with cause, both chequebooks, "FOREIGN
COMMITMENTS ARE ESTIMATES") and THE COLONIAL QUESTION (open windows).
The Intelligence panel (`I`), which was never registered in the app,
is wired in with this slice since the fast verbs consume it. No
percentages anywhere; the Kent legend appears once, in the panel
footer.

## Determinism

Integer positions in BTreeMaps; forks `b"elections"`, `b"coup"`,
`b"influence-blown"`; the allocator reads a snapshot and writes deltas
in tag order; monthly work on `new_month`, elections by date compare
with a fired set; `Influence::digest()` folds everything except the
wire and chequebook; added to the determinism snapshot, the savegame
replay digest (with `Intel`, which was missing), `SimPlugin::build`,
and `reset_sim`. Tests: seeded Japan/FRG/Poland bands on tick one;
aid moves a target and withdrawal reverses it; hysteresis keeps a
0-position oscillation from flipping; a coup ladder with fixed inputs;
an election calendar row fires once; Independence opens and closes a
window; SetAlignment is a no-op inside the band; same-seed bit-identity
with programs and ops active; the 20-year hands-off run keeps its
spine with anchor bands checked at 1955/1960/1965.

## Implementation status (v1, 2026-09-02)

Shipped in `crates/ugs-sim/src/influence.rs` (+ `influence_ui.rs`,
`MapMode::Influence`, two newspaper sections) with
`assets/data/scenario/1950/influence.ron` (123 sourced seed rows, 41
battlegrounds over six regions, 66 elections) and
`events/09-influence-1950-1970.ron` (unlocks, treaty locks, the Soviet
economic offensive, Belgrade). The Hungary, Ghana, Guinea, Alliance for
Progress, NATO-accession and France-exit events were retargeted onto
the new effects; the Sino-Soviet treaty now locks Peking until the
split.

Deviations from the design above, all deliberate:
- **Diminishing returns** were added: a program on a target already
  inside the sponsor's band runs at half rate. The hands-off harness
  showed an unopposed presence program painting Syria to +569 by 1965
  without it.
- **Elections** fire when the calendar date is reached (any hour), not
  at hour 12, so a save loaded mid-day cannot skip one.
- **Independence-born states** carry the seed row's position as their
  birth lean (±150 or 0) rather than a separate `baseline_pole` field;
  a program placed on the announced tag accumulates on top of it.
- **The recognise-the-junta choice** is flavour with a small price
  (legitimacy −3, +50 lean), not a distinct mechanic.
- **The AI never funds aid it cannot afford**: it keeps a 900-centi
  reserve and falls back to presence, so in practice Moscow and
  Washington mostly run presence programs; AI aid appears once pools
  are large (Vietnam, the Congo).
- **Coup frontier** prints the tension band word, not the region gate,
  since the gate is already the disabled reason on the button.
- **Projection timing**: scripted shoves, coups, locks, clauses and
  independence project the band immediately; program flows, elections
  and released occupation zones write the position and let the band
  catch up at the month rollover (the hysteresis rule is evaluated
  there). Coup gates are re-checked at resolution: a lock placed or a
  war declared during the ninety days stands the operation down.
- **Seeding** happens before the first command flush and at app boot,
  so the paused 1950-01-01 screen already shows slots, locks and
  positions. Seeded `until_year` locks lapse on the real calendar;
  event-granted locks and contest windows count 30-day months.

Calibration: `hands_off_anchors_land_near_history` runs fifteen years
hands off and checks forty (year, country, band) anchors with at most
three mismatches, plus every 1950 NATO member and Cominform satellite
still in band at 1965. The ignored `hands_off_bands` diagnostic prints
the full anchor table.

## Cuts (deliberately not modelled in v1)

Living-standard drift; neutral gravity; adjacency realignment and reach
rules; the Government/Army/Street triad; the Aswan auction and NAM
actor verbs; the metropole as a distinct bidder; the PRC pole; AI
covert ops; the attribution three-state machine (ops branch on the
blown boolean only); the UN General Assembly; the CIVIL WAR rung;
the briefing recommendation line; per-region street lean.

## Post-slice, in return order

1. Attribution machine + crisis pretext API (shared with espionage
   v1.5); flashpoints read alignment.
2. AI election pushes and coups through the same allocator.
3. The triad behind a written decision test.
4. The auction: aid bids as placed projects, a recipient decision
   rule, NAM verbs for playable middle powers.
5. Metropole bidder and the PRC pole after the split.
6. Extended election calendar, cadre pipelines.
7. UNGA as a second scoring surface.
8. The systems-competition term as a checkpoint score line.

## Resolved questions (from the sketch)

- **Non-Aligned**: an active attractor, shipped as a field with a score
  (denied counts, Bandung/Belgrade pull, highest inertia) now and as an
  actor with the auction later. Never a third AI.
- **Granularity**: per-country position plus dynamic stability plus an
  army-patron flag. That is enough to make aid, presence, election push
  and coup four distinct verbs. Factions return only if a decision
  cannot be expressed without them.
- Rejected: a fungible influence bank (snowballs); a per-country cash
  aid verb (the bank by the back door); crush via occupation zones
  (zones are war-derived and holder-relative); world opinion as a
  second ledger (legitimacy already exists and is spent).
