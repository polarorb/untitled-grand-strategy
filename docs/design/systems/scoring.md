# Era Scoring & the Verdict

Status: implemented (v1 slice; deviations noted below)
Pillar: serves all four — it is the answer to "how do I win the
century without ending the world?" (vision.md, Victory). Reads pillar 2
(the frozen standings), pillar 4 (output), the settlement table
(legitimacy, treaties), and pillar 1 (the catastrophe state).
Research: [scoring](../../research/scoring.md) (6-analyst swarm, two
skeptics, synthesis, 2026-09-03).
Siblings: [influence](influence.md), [escalation](escalation.md),
[war-termination](war-termination.md), [newspaper](newspaper.md).

North star: **the record is kept in words, at four dates, and the
exchange has no winner.** One formula for every nation; the map scored
as a delta from 1950 in the nation's own column; catastrophe a state
above the score, never a term inside it.

## Player-facing description

- **THE STANDING**: one line in the paper's WORLD IN NUMBERS rail every
  month — "OUR STANDING: SLIPPING — ASIA RUNS AGAINST US [P] — NEXT
  RECKONING JAN 1955 (54 MONTHS)". A word (GAINING / HOLDING /
  SLIPPING) from the provisional era score with a dead band, the term
  that moved it most, and the key of the panel that owns that term. A
  one-word HUD chip mirrors it. Bloc leaders get a second line, HEAD TO
  HEAD, with a Kent qualifier; when the rival's estimate bracket is
  wider than the margin it reads EVEN. No number appears anywhere.
- **HOW THE CENTURY IS SCORED**: a box in the founding edition, period
  prose, once: four terms, four dates, the non-aligned count against
  both, catastrophes are never forgiven, an exchange ends the record.
- **THE RECKONING**: at each freeze (1 Jan 1955, 1960, 1965, 1970) a
  teletype special pauses the game with an 80-word card, and the paper
  carries the full page until the next: WHERE WE STAND (four rows ×
  OURS / THEIRS (EST.) / VERDICT), THE MAP BY REGION (six verdict words
  with counts, the narrowest margin marked THE PRIZE), MOVERS (up to
  six battlegrounds with "BAND SINCE DATE — CAUSE"), AS THEY SEE IT (the
  rival's claim from its reported figures), the era grade ("NARROW
  GAIN"), and the identity line "THE ERAS SO FAR SUM TO THE BOARD SINCE
  1950". Later monthly papers print "ON THE RECORD SINCE THE {year}
  RECKONING" beneath the live line.
- **THE FINAL EDITION** at 1970-01-01: a masthead class word (WON / HELD
  / LOST / COSTLY), one verdict sentence per byline (own organ, rival
  organ, the non-aligned Gazette), THE THREE THINGS THAT DECIDED IT
  (dated lines pointing at an era term or a fired event), the four era
  cards as a grid of words, and — only here — BELIEF BESIDE THE RECORD
  for the rival's columns and the planner's own reported-versus-actual
  output. PLAY ON continues the timeline with no further reckonings.
- **The exchange**: the funeral screen is kept verbatim and gains one
  line, the date of the last paper. No ledger, no class, for anyone.
- **Minors**: the paper says plainly which terms a zero-slot nation can
  move ("YOUR LEDGER MOVES ON OUTPUT AND STANDING [E] [T]"); scale 3
  makes those decisive.

## State

`Ledger` resource (`crates/ugs-sim/src/score.rs`):

| Field | Type | Meaning |
|---|---|---|
| `par` | `BTreeMap<CountryTag, Par { board: i32, legband: i32, ipc: u64, pop_k: u64 }>` | frozen at seed, tick 0 |
| `eras` | `Vec<Era { year, cards: BTreeMap<CountryTag, Card>, brink_months: u16 }>` | pushed at each freeze |
| `provisional` | `BTreeMap<CountryTag, Card>` | recomputed monthly since the last freeze |
| `end` | `Option<CampaignEnd>` — `Reckoning(1970)` or `Exchange { initiator, tick }` | |
| `brink_months` | `u16` | months the world sat at Brink this era |
| `last_word` | `BTreeMap<CountryTag, (Word, u8)>` | the standing word and how many consecutive months it has agreed (the arrow rule) |
| `seeded` | `bool` | |

`Card { map, output, standing, peace: i32, catastrophe: Catastrophe,
cause: Cause, board: i32, ipc: u64, ipc_reported: u64, legitimacy: i32,
dead: u64, uses: u16, treaties: u8 }` — the four terms plus the exact
inputs the 1970 reveal prints. `Catastrophe { Unscarred, Scarred,
Exchange }`. `Cause` is a discriminant plus an id (a region name, a
tag, an event id), never a rendered string; the app templates it.

Attribution fields added beside their mechanics, all digest-folded:
`NuclearPrograms.first_use: Option<(CountryTag, u64)>` and
`NuclearPrograms.uses: BTreeMap<CountryTag, u16>`; `Crises.prevailed`
and `Crises.stood_down: BTreeMap<CountryTag, u16>`; `Treaty.proposer`.
`Military::digest` additionally folds `casualties`, `battles_won`,
`battles_lost` and `wars` (they were divergence-invisible).

Scenario data, in `assets/data/scenario/1950/influence.ron`:
`region_values` (per region: the Presence / Domination / Control point
values — Twilight Struggle's, Europe Control capped at 12) and
`scorecards` (per playable nation: reach regions and scale; default
own region, scale 3). `BattlegroundDef.weight` stays allocator
priority, never a score value.

## Cadence and stage

`update_score` is the first and only system in `TickSet::Resolve`, so
it sees this month's standings, production and legitimacy. Seeds the
par on the first tick (through `ensure_seeded`, before the first
command flush). Monthly: recompute the provisional cards and the
standing words; count a Brink month. Freeze when
`influence.checkpoints.len() > ledger.eras.len()` (the influence
`CHECKPOINT_YEARS` extends to 1970); the 1970 freeze sets
`end = Reckoning`. Every tick: if a `GameOver` resource exists and
`end` is not `Exchange`, set it and stop updating cards.

## Formulas (`score::tuning`)

All integers. Per region and column, `points(r, c) = REGION_VALUES[r]
[verdict(count_c, max of the other two columns, thresholds_r)] +
count_c`. A pole's board is `Σ over its reach of points(r, own pole) −
points(r, rival pole)`; the field's board is its points minus the
stronger pole's. The column is the nation's live band at the freeze
(DENIED for the non-aligned field). The field denies verdicts inside
`points` (a large non-aligned count blocks Domination) but a state
born non-aligned moves neither pole's board.

- `MAP = board(now) − board(previous freeze)` (the par for 1955).
- `OUTPUT = clamp((growth − comparator) / OUTPUT_STEP, −OUTPUT_CAP,
  +OUTPUT_CAP)` where growth is the permille change since the previous
  freeze in industry per thousand people (`actual_centi × 1000 /
  pop_k`), the comparator is the rival pole leader's growth for a bloc
  member and the world median growth for the field; `OUTPUT_STEP = 50`,
  `OUTPUT_CAP = 4`. Gated out of the total while `OUTPUT_GATED` holds
  (see Implementation status).
- `STANDING = clamp(legband(now) − legband(previous), −3, +3)` with
  `legband = clamp(legitimacy / STANDING_STEP, −4, +4)`, `STANDING_STEP
  = 30`.
- `PEACE = min(PEACE_SETTLED × treaties this era, PEACE_SETTLED_CAP) −
  min(own dead × 10,000 / pop1950 / PEACE_DEAD_UNIT, PEACE_DEAD_FLOOR)
  − PEACE_FIRST_USE × attributed uses − (pole leaders only)
  min(brink months, PEACE_BRINK_FLOOR)`; `PEACE_SETTLED = 2`, cap 4,
  `PEACE_DEAD_UNIT = 2`, floor 8, `PEACE_FIRST_USE = 4`, brink floor 4.
  "Treaties this era" counts executed treaties the nation signed that
  ended a war it was party to.
- `S = MAP + OUTPUT + STANDING + PEACE`. Era grade on `|S × scale|`:
  0-1 STALEMATE, 2-5 NARROW, 6-11 CLEAR, 12+ DECISIVE, with GAIN or
  LOSS from the sign.
- Catastrophe for the era: EXCHANGE if the campaign ended in one;
  SCARRED if the nation has an attributed use this era or its own dead
  this era reach `SCARRED_DEAD = 30` per 10,000 of its 1950 population;
  else UNSCARRED.
- Campaign `C = Σ S` over the frozen eras. Class on `C × scale`: WON ≥
  +10, HELD −9..+9, LOST ≤ −10; any SCARRED era caps the class at COSTLY
  (printed with the grade and the bill); EXCHANGE means no class for
  any nation on earth.
- Standing word (monthly): `|provisional S × scale| ≤ 1` → HOLDING,
  else GAINING or SLIPPING by sign; the arrow appears only after two
  consecutive agreeing months. Head-to-head for the two poles:
  `sign(S(USA) − S(SOV))` with dead band 2, EVEN when the rival's
  output bracket exceeds the margin.

Epistemics: the sim stores truth in the card. Every rendering before
1970 goes through the viewer: own output through
`dashboard_industry_centi` (REPORTED for planners), the rival's
through `observed_industry_centi` at economic penetration as a
two-significant-figure bracket with a Kent word.

## Interactions

Reads: `Influence.checkpoints`/`standings` and `alignment_of`
(column), `Economies.industry` and the population map (output),
`Settlements.legitimacy` and `treaties` (standing, peace),
`Military.casualties` (dead), `NuclearPrograms.first_use`/`uses`,
`GlobalTension.band` (brink months), `GameOver` (exchange). Writes:
nothing outside `Ledger`. The AI never reads the ledger. The only bite
on play is that legitimacy is both the STANDING term and the currency
spent at the settlement table, credited back under PEACE.

## Edge cases

- A nation born after 1950 has no par: its par is taken at birth
  (first freeze after it owns provinces) and its earlier eras print
  "NOT YET ON THE RECORD".
- A nation absorbed or with zero population: OUTPUT 0, no division by
  zero; the card prints "NO RECORD".
- Column changes between freezes (France leaves NATO, Cuba goes East):
  the delta is computed in the column held at the later freeze at both
  ends, so a defection reads as the board it now faces.
- A post-1970 exchange still replaces the verdict with the funeral.
- A player who changes nation mid-campaign is scored as the nation
  they hold at each freeze.

## AI note

None. The ledger is read-only for the AI in v1 by design; see the
research on the reverse thermostat.

## Presentation

Three paper sections (THE STANDING line, THE RECKONING page, THE FINAL
EDITION), a HUD chip word, the founding-edition box, the one funeral
line. The reckoning and final edition also arrive as teletype notices
(which pause the game) so the freeze is a moment. No panel, no chart.

## Determinism

Integer math over BTreeMaps; no RNG; monthly on `new_month`; freezes
keyed to the influence checkpoint predicate; `Ledger::digest()` folds
par, eras, end, brink months and the standing words; added to the
determinism snapshot, the savegame replay digest, `SimPlugin::build`
and `reset_sim`. Tests: the par seeds at tick 0; a scripted treaty
credits PEACE; a scripted coup's legitimacy shows in STANDING; MAP
identity (Σ era MAP = board delta since 1950); the hands-off harness
`hands_off_verdict` asserts bands — neither pole CLEAR at 1955, both
HELD, East Σ MAP ≥ +10, both UNSCARRED, every nation classed — and a
same-seed run reproduces byte-identical cards.

## Implementation status (v1, 2026-09-03)

Shipped in `crates/ugs-sim/src/score.rs` as the first system in
`TickSet::Resolve`, with `region_values` and `scorecards` in
`influence.ron`, the attribution counters beside their mechanics, and
three paper surfaces plus the HUD chip and the funeral line.

Deviations from the design above, all deliberate:
- **OUTPUT is gated** (`tuning::OUTPUT_GATED`). The hands-off economy
  grows Soviet industry per head about ten times faster than American
  in every era (+157/+272/+303/+342 permille against +16/−31/+49/+93)
  with no 1960s slowdown, so the term alone would decide every
  campaign. It is computed, stored on the card and printed as context
  ("CONTEXT ONLY") but excluded from the era total until the economy
  doc calibrates growth (post-slice item 1).
- **The board rule** subtracts the rival pole only: a pole's board is
  its points minus the other pole's; the field's board is its points
  minus the stronger pole's. Subtracting the best other column
  penalised both poles whenever a newborn battleground was born
  non-aligned (the 1955 East read −8 with nothing flipped).
- **Class band**: HELD spans −9..+9 at scale 1 (WON ≥ +10, LOST ≤
  −10), so the timeline's Third World gains (East MAP +7 hands off)
  read as HELD for both poles rather than a Soviet WON.
- **STANDING step** is 30 legitimacy per band point, so the scripted
  UNSC 83 grant (+25) is not a point on its own.
- **MOVERS** are not printed on the reckoning page yet (the wire ring
  is transient); THE MAP BY REGION and AS THEY SEE IT are.
- **The reckoning** is a teletype notice (which pauses) plus a page in
  the monthly paper that stays for a year, not a separate edition.
- **Own dead** are the nation's military casualties; civilian dead from
  tactical use are not yet attributed per country.

Hands-off (seed 1950), the era cards for the poles read: 1955 West +5
NARROW GAIN (Iran, Pakistan, Korea consolidated), 1960 East +7 CLEAR
(Egypt, Iraq, Guinea), 1965 East +7 CLEAR (Cuba, the Congo contested),
1970 STALEMATE; both poles HELD, both UNSCARRED, the US Korean dead
73,000 on the record.

## Cuts (deliberately not modelled in v1)

Expectation rows; a hands-off par; write-backs; a score-reading AI;
OUTPUT as level; SCARRED from crushes; cohesion, credibility, firsts
and UNGA terms; the WHO LOST X debit; the provocation ledger; a
difficulty par bid; DIVERGENCES and the hall of records.

## Post-slice, in return order

1. Economy calibration gate for OUTPUT (planned out-growth in the
   1950s, slowdown after 1963).
2. Crush priced in legitimacy inside the influence mechanic.
3. Minor-power alignment verbs.
4. Tension authorship and the provocation ledger.
5. Contestability state and the capped credibility debit.
6. Score-aware allocator, argued in the influence doc.
7. Par bid for difficulty and multiplayer.
8. Great Project races as FIRSTS; readable lock strength as COHESION.
9. Counter-cyclical write-backs once a regime model exists.
10. UNGA flavour; DIVERGENCES; the hall of records.
