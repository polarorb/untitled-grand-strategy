# Escalation & Nuclear Brinkmanship

Status: designed (research locked; implement in phases below)
Pillar: 1 (primary), touches all others
Research: [nuclear-weapons](../../research/nuclear-weapons.md) (5-analyst
swarm) · [tension implemented](tension.md)

North star: **the bomb is the gun on the wall that everyone must point
and nobody may fire.** Total war is the failure state. Every mechanic
below exists to make staying under the threshold the game.

## Architecture at a glance

Four connected layers, all deterministic, all on existing infrastructure:

1. **The program** — an industrial project (uranium → grid-hungry
   plants → fissile kg/month → warheads) with visible milestones and
   spy-observable signatures.
2. **The arsenal** — stockpile vs *assembled*, delivery legs with
   range-vs-basing checks, per-dyad deterrence classes computed from the
   rival's *estimate* of you.
3. **The crisis ladder** — per-crisis 8-rung ladders with three hard
   firebreaks, incident hazard while at altitude, resolve as bluff
   currency. The global tension meter is background weather.
4. **Use & the end** — tactical use as the designed trap (taboo break),
   strategic exchange as an attributed, unrewarded campaign end.

## 1. The program

Six stages per nation, state advanced on `new_month`, all integer math:

0. **Found program** — secret choice event; sets funding tier.
1. **Establishment** — site selection + scientist roster (5-15 named,
   skill 1-5, 0.5× stacking decay; recruitable, defectable,
   assassinatable).
2. **Route choice** (mostly irreversible, advisor-argued choice event):
   - *Plutonium*: Production Reactor + Reprocessing Plant, low grid
     draw, requires the Implosion Design sub-project (scientist-gated,
     fizzle-risk without stolen data).
   - *HEU*: Enrichment Complex, 2-3× industrial cost, 15-25% of a
     region's grid per level, no design risk (gun-type).
   - Superpowers may run both at ~1.8× total cost.
3. **Fissile production** — kg/month = f(plants, uranium feed t/month,
   grid supply ratio). Uranium comes from controlled deposits or import
   agreements; denial (sabotage, interdiction, flipping supplier
   states) is first-class below-the-threshold play.
4. **First device** — test-or-stockpile decision. Testing is public
   within days regardless of secrecy (atmospheric sampling — RDS-1
   precedent) and fires the global teletype arc.
5. **Arsenal / thermonuclear** — follow-on projects (H-bomb, sealed
   pits ~1957 removing the assembled-vs-stockpile distinction,
   boosted designs).

**Program posture** (3-position): Covert 0.7× speed / Standard /
Crash 1.5× with high detection accrual, accident-event risk, and
consumer-goods pressure. Detection is a monthly accumulator fed by grid
signature + procurement + enemy penetration; thresholds upgrade the
rival's estimate tier.

**Espionage coupling**: penetration sharpens your estimate of them and
yields stolen-design modifiers (−20-35% on design sub-projects, removes
fizzle risk — the Fuchs effect, worth ~2 years). Counterintel burns
networks with public spy-trial events. Facilities are sabotageable and
conventionally bombable.

## 2. Arsenal & deterrence

- Warheads assigned to delivery legs — bombers (1950), missiles
  (late 50s), SSBNs (~1960) — each with survivability and penetration
  permille. Range-vs-basing checked against the real province map: in
  1950 the USSR threatens Europe/Japan but can hit CONUS only via
  one-way Tu-4 raids; the US needs its overseas base ring. Basing
  rights are diplomacy/influence content.
- Monthly per-dyad computation: `FirstStrikeDamage` and
  `AssuredRetaliation`, classifying the dyad **NONE / ONE-SIDED /
  MUTUAL / ASSURED**. Effects: under MUTUAL+, declarable war between
  the peers is removed (reachable only by climbing a crisis ladder) and
  crisis/proxy frequency rises (stability–instability paradox). Under
  ONE-SIDED (1950), conventional war under an atomic shadow — Korea as
  is.
- **Both sides compute deterrence from intelligence estimates, not
  truth.** Estimates carry a hidden bias term (historically 2-10× high)
  plus the existing 2-sig-fig fuzz; parade deception is a covert action
  that inflates the enemy's estimate of you (deterrence now, arms-race
  blowback later). Collection tech (U-2-era overflights 1956+,
  satellites 1960+) tightens ranges and carries its own crisis risk.

## 3. Crises

Crisis = ECS entity ticked in the Politics stage, iterated in id order:
`{ stake, initiator, target, rung, deadline_ticks, committed[2] }`.
Delivered entirely through the teletype choice-event engine.

Ladder (8 rungs, 3 firebreaks — firebreaks crossable ONLY by explicit
choice events):

    R1 Diplomatic protest
    R2 Show of force
    R3 Mobilization / blockade
    R4 Conventional clash
    ── FIREBREAK A: shooting war ──
    R5 Open conventional war
    R6 Nuclear alert + public ultimatum
    ── FIREBREAK B: nuclear use ──
    R7 Demonstration / tactical use
    ── FIREBREAK C: homeland strike ──
    R8 General exchange  (campaign over)

- Global tension caps opening rungs (tension <30 → crises open R1-2;
  >70 → can open at R4) and every rung climbed anywhere adds +2..+8
  global tension.
- Options each round: ESCALATE / HOLD (deadline pressure — the last
  ultimatum-issuer must escalate or fold at expiry) / BACK DOWN
  (opponent takes the stake; resolve cost scales with rungs *you*
  climbed: 2/rung at R1-3, 6 at R4-5, 12 at R6+) / COMPROMISE (both
  spend a little resolve, split the stake).
- **Incident hazard**: basis points per hour at rung ≥3 (R3: 1, R5: 4,
  R6: 12), multiplied by both sides' alert levels, rolled from
  `rng.fork(b"crisis")` with the crisis id in the label. Incidents
  (shoot-down, naval collision, radar ghost) auto-climb one rung unless
  someone immediately folds on worse terms.
- **Resolve** 0-100 per nation: own shown exactly; enemy's only as a
  fuzzed estimate narrowed by covert ops. Winning a crisis pays more
  the higher the loser folded.

## 4. Postures (four dials, all logged commands)

1. **Alert level** (Peacetime / Increased / Airborne / Maximum): money
   + fuel per tick, +3/+8/+15 tension while held, incident hazard
   ×1/×2/×4, cuts response penalty if struck. Enemy sees it fuzzed,
   6-24 ticks late — alerting IS signaling.
2. **Declaratory policy** (No-First-Use / Ambiguity / Massive
   Retaliation): baseline tension −5/0/+8; NFU deletes the R6 option
   but pays non-aligned influence; MR makes low-rung threats credible
   cheaply but ratchets crises.
3. **Delegation** (National / Theater-request / Predelegation):
   predelegation shortens response but makes the false-warning event
   auto-launch — the player can configure their own doom.
4. **Targeting doctrine** (counterforce / countervalue): shifts
   first-strike math vs deterrence credibility.

## 5. Use, the taboo, and the end

- **The MacArthur chain**: war + front collapse + capability + theater
  delegation → commander requests release. Refuse (loyalty cost,
  eventual insubordination arc), demonstration shot (+25 tension,
  patron crisis at R6), or battlefield use — which *wins the battle*
  and breaks the global taboo: a permanent one-way flag that drops
  everyone's use thresholds forever, collapses neutral alignment, and
  hands the enemy patron a forced crisis.
- **No nuke button.** Every use is a choice event with two-man-rule
  friction: consequences restated in plain teletype, confirmation
  phrase, recallable until H-hour (bombers; missiles later are not).
- **Failure state** — general exchange only via a crisis at R7-8 plus a
  final human choice, or a courted accident (max alert + predelegation
  + false warning). Never from the meter alone; always reconstructible
  from the command log. Final retaliation is unconditional. The end is
  non-interactive: grid regions go dark in sequence, the teletype
  prints real city names and real populations, degrades, and dies —
  then one ledger: exact dead, the attribution chain, and how long the
  peace held. No aftermath play. Load menu only.

## 6. Presentation

- **Strategic map mode** (N): dark phosphor big-board restyle; locked
  behind "NO STRATEGIC FORCES" pre-program. Coverage washes from named
  bases; hatched SUSPECTED overlays for enemy reach; target folders
  (Courier Prime) with real populations.
- **Tier 1 (always visible)**: tension + band, own posture chip, own
  stockpile odometer ("STOCKPILE: 0017" — low numbers are the drama),
  NIE-revision flag.
- **Own program**: dated milestone dossier with exact figures and a
  running cost line. No progress bars — dates and physical quantities.
- **Enemy programs**: NIE cards — "EST FIRST TEST: 1952-54 ·
  CONFIDENCE: LOW · LAST HUMINT: 14 MONTHS AGO" — allowed to be wrong;
  revisions fire wire events.
- Own test: countdown → flash → silence → yield estimate → announce or
  conceal. Enemy test: multi-day detection arc (sampling flight →
  review → announcement), with your busted prior estimate displayed.
- Voice: clinical bureaucratic ALL-CAPS, numbers over adjectives,
  classification stamps, codenames for everything.

## Tuning anchors (sourced — see research doc)

US stockpile 299 (1950) → 2,422 (1955); USSR 5 → 200; superpower crash
program ≈ 4 game-years, middle power 6-8; complex ≈ one year of
great-power military-industrial output; delivery costs 5-10× warheads;
USSR two-way CONUS strike ~1956; Polaris 1960 = survivability
breakpoint. Event spine: H-bomb decision Jan 31 1950 · Fuchs arrest
Feb 1950 · Korea custody transfer Apr 1951 · Castle Bravo Mar 1954 ·
Massive Retaliation Oct 1953 · Open Skies Jul 1955.

## Implementation sequencing

1. **Programs & arsenals v1**: NuclearPrograms resource (stages, route,
   posture, fissile kg, stockpile), facilities drawing real grids and
   uranium, scientist roster, detection accumulator, H-bomb and Fuchs
   events, first-test arcs, HUD strip + program dossier + NIE cards.
2. **Deterrence & delivery**: legs, range-vs-basing, dyad classes,
   estimate bias + parade deception, strategic map mode.
3. **Crisis ladders**: Crisis entity, incident hazard, resolve,
   band-gated openings, first authored crises (Berlin, straits).
4. **Use & endgame**: MacArthur chain in the Korea slice, taboo flag,
   alert/delegation dials, the failure sequence.

## Resolved questions (from the sketch)

- Tension stays one global meter; crises are the per-dyad state.
- Third parties read tension through crisis frequency and alignment
  drift (pillar 2 hooks).
- Crisis pacing: auto-pause choice events with tick-count deadlines —
  no real-time twitch.
