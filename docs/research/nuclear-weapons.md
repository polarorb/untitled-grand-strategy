# Nuclear weapons: the program, the brink, and the bomb that must not be used

*Research swarm, 2026-08-25 — five analysts on acquisition mechanics in
the genre, deterrence & use design, sourced historical numbers, war-room
UI, and crisis/brinkmanship mechanics. Raw findings in
[nuclear-weapons-raw.json](nuclear-weapons-raw.json).*

North star from the vision doc: *"the bomb is the gun on the wall that
everyone must point and nobody may fire."* Total war is the failure
state; this pillar is the game's identity. The five reports converged
to an unusual degree — the synthesis below feeds the
[escalation design doc](../design/systems/escalation.md).

## The convergent principles

**1. Acquisition must be an industrial story, not a research unlock.**
HoI4's classic chain (3 techs → reactor building → 1 bomb per 852 days)
is the genre's cautionary tale — a decisionless drip with no world
reaction, which Paradox itself repudiated in its 2024 rework (named
scientists, raidable facilities, staged prototypes). Historically >80%
of the Manhattan Project's $1.89B went to fissile-material plants —
Oak Ridge alone was 63% — and the physics was the cheap part. Nobody in
the genre has shipped an acquisition chain that is itself a game.

**2. We are uniquely positioned to model it honestly.** Enrichment is
electricity-monstrous (K-25 had a dedicated 238 MW plant; by the
mid-50s the AEC drew ~6-12% of US electricity) — and we already
simulate regional grids. Uranium came from named chokepoints we already
have as deposits (Shinkolobwe at 65% ore grade supplied the West; the
USSR lived off Wismut and Joachimsthal tonnage after being locked out
of the Congo). Grid buildout is simultaneously the production
bottleneck *and* the detection signature of a "secret" program.

**3. The route choice is the program's central decision.** Plutonium:
cheap reactors, modest power draw, but the hard implosion design (the
sub-project espionage de-risks). HEU: brutally expensive and
grid-hungry, but the simple gun-type that needs no test. Britain and
the USSR each agonized over exactly this; superpowers may run both at
~1.8× cost. One decision differentiates US/USSR/UK playthroughs.

**4. Deterrence keys off the rival's *estimate*, not the truth.** The
historical record of strategic intelligence is *directionally biased
overestimation* — the bomber gap (~10× high), the missile gap (25-100×
high: four real Soviet ICBMs in 1961 against NIEs of hundreds) — driven
by deliberate parade deception plus worst-case extrapolation. So:
estimates carry a hidden bias term, deception is a playable covert
action (with arms-race blowback — phantom fleets drove real
procurement), and bluffing a deterrent you don't have becomes possible.
Espionage narrows the range; this is the bridge between the intel
pillar and this one.

**5. Keep Kahn's firebreaks, not his 44 rungs.** An 8-rung per-crisis
ladder with three hard thresholds — shooting war, nuclear use, homeland
strike — each crossable only by an explicit choice event, never by a
drifting meter. The global tension meter is background weather: it caps
where new crises *start* and absorbs every rung climbed anywhere.

**6. Make Schelling literal: time at altitude is the weapon.** While a
crisis sits at rung R, a deterministic incident hazard (basis points
per hour, multiplied by both sides' alert levels) can climb the ladder
*for* you — Black Saturday's three unordered near-disasters as a
mechanic. Enemy resolve is shown only as a fuzzed estimate; backing
down costs resolve proportional to rungs *you* climbed, so early
concession is cheap and late concession is political catastrophe.
Brinkmanship pays — that's why players will voluntarily approach the
failure state.

**7. Nuclear capability changes the rules, not just numbers.** Snyder's
stability–instability paradox: per-dyad deterrence class (none /
one-sided / mutual / assured) removes declarable war between nuclear
peers while *raising* crisis and proxy frequency. In 1950 deterrence is
one-sided and Korea plays conventionally under an atomic shadow —
historically correct for free.

**8. Tactical use is the designed trap.** The MacArthur chain: at
moments of conventional collapse the theater commander *requests* the
bomb, and it genuinely wins the battle. The costs are systemic and
irreversible — tension +25-40, a permanent global taboo-break that
cheapens all future use for everyone, alignment collapse among
neutrals, a forced patron crisis. The player should want it and refuse
it (Tannenwald's taboo, mechanized).

**9. Never a unit order; always a political event.** No nuke button on
the map (HoI4's failure — bomb as siege artillery). Every use passes
through the choice-event engine with two-man-rule friction: restated
consequences, confirmation phrase, a recall window (bombers turn back;
missiles later can't).

**10. The failure state rewards nothing.** Crawford's rule ("we do not
reward failure"), Twilight Struggle's attribution (the trigger-puller
loses), DEFCON's megadeath accounting. General exchange = immediate
campaign end: grid lights go out region by region, the teletype prints
real city names from our real demography, garbles, and dies —
"TRANSMISSION ENDS" — then one ledger: exact dead, the attribution
chain from the command log, and the date the peace failed. Score is how
long you kept it. No aftermath gameplay, ever. Final retaliation is
unconditional — there is no splendid first strike.

**11. The UI is the Strangelove big board.** A dark phosphor
"Strategic" map mode (locked behind "NO STRATEGIC FORCES" until you
have a program); own program as an exact dated milestone dossier wired
to real inputs; enemy programs as NIE cards — range + confidence word +
estimate age — that are allowed to be *wrong*, Joe-1 style; a 5-detent
alert lever the player owns; reach as phosphor coverage wash from named
bomber bases (basing rights = diplomacy content). Tier 1, always
visible: tension, own posture, stockpile odometer. Clinical
bureaucratic voice everywhere; numbers over adjectives; no Fallout
kitsch.

## Tuning anchors (sourced)

- **Stockpiles** (Norris & Kristensen, Bulletin of the Atomic
  Scientists 2010): US 1945: 2 → 1950: 299 → 1955: 2,422 → 1960:
  18,638. USSR 1950: 5 → 1955: 200 → 1960: 1,605. UK single digits from
  1953. A 60:1 ratio at start collapsing to ~12:1 by 1955; USSR runs
  the US curve ~8-10 years behind.
- **Ready vs stockpiled**: 1950 US cores sat in civilian AEC custody,
  unassembled — ~2 days per weapon, few assembly teams. Model
  stockpile ≠ assembled until the sealed-pit transition (~1957), which
  unlocks the 1958-59 production explosion.
- **Program scale**: superpower crash ≈ 4 game-years (US 3.1, USSR ~4);
  determined middle power 6-8 (UK 5.7, France ~6, China ~9). Complex
  costs ≈ a year of great-power military-industrial output. Warheads
  cheap once plants run; delivery costs 5-10× the warheads (56% vs 7%
  of all US nuclear spending).
- **Delivery**: USSR cannot two-way-strike CONUS until Tu-95/M-4
  (~1956) — Tu-4 one-way raids only. US needs the overseas base ring
  (UK, Morocco, Guam) for its B-47 backbone. Early ICBMs are prestige
  terror with hours-long fueling and a handful of soft pads; Polaris
  (Nov 1960) is the survivability breakpoint that ends first-strike
  calculus.
- **Event spine 1950-55**: H-bomb decision (Jan 31, 1950 — day 31 of
  the campaign; Fuchs confessed Jan 27), Korea atomic custody transfer
  (Apr 6, 1951, days before MacArthur's relief), Castle Bravo yield
  overshoot (Mar 1, 1954 — 6 Mt predicted, 15 delivered; spawns the
  test-ban pressure track), Massive Retaliation (NSC 162/2, Oct 1953),
  Open Skies (Jul 1955).

## What this changes about existing systems

- Tension gets band-gated action locks/unlocks and an attribution
  ledger (who pushed it over each threshold).
- The intel-estimate language (already shipped for armies) gains a bias
  term and NIE presentation; estimate *revisions* become wire events.
- The economy grows facility construction that competes with everything
  else for industry and eats named regional grids.
- Korea becomes the tactical-use temptation testbed.
