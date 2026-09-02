# Ideology & influence warfare — research readup

Researched 2026-09-02 by a 7-analyst swarm, two adversarial skeptics,
and a synthesis pass: genre influence mechanics (Twilight Struggle,
Balance of Power, Vic2/Vic3 spheres and lobbies, HoI4, Terra Invicta,
Suzerain/Kremlingames), the board-game contest model as pure
arithmetic, a Cold War historian on the 1947-66 influence toolkit, the
decolonization bidding window, the domestic mirror and bloc
discipline, legibility and the decision budget, and a codebase-
grounded systems-integration analyst. Raw:
[influence-raw.json](influence-raw.json).

Design brief: pillar 2 is the only pillar still at `sketch`. Alignment
exists as static 1950 data plus timeline overrides; nothing moves it
except scripted events, and the 37 newborn states of the decolonization
wave are assigned their alignment by the event file. Make "the map is
painted by alignment" a contest the player and the rival AI both play,
without a 226-country whack-a-mole and without breaking the hands-off
1950-1970 timeline.

## Convergent principles

1. **One signed position per country, and the enum is derived from
   it.** A `-1000..+1000` East↔West position (the occupation-zone
   unit) owned by a new `Influence` resource and projected into the
   existing `alignment_of` accessor, so bloc basing, patrons, red lines
   and the masthead never change. Scripted `SetAlignment` becomes a
   band-edge shove (no-op if already in band); events gain
   `ShiftAlignment` and `LockAlignment`; a new `AlignmentBand` trigger
   lets chains branch on who won. Nothing sets bloc directly — a
   country changes bloc because its position crossed a band, and the
   paper prints the cause. (Every genre "Set Alignment" verb shipped
   was hated; the history skeptic verified band-edge semantics keep
   the timeline tests passing where deltas would not.)
2. **Influence is priced in attention slots plus escalation headroom
   plus politics — never a fungible bank.** Every system that let a
   superpower buy alignment with cash snowballed (HoI4 political
   power, Superpower 2); every one that priced it in capped slots and
   escalation felt right (TS ops, Vic2 focus slots, EU4 envoys). Aid
   lives *inside* a slot and draws from the construction pool, so
   pillar 4 funds pillar 2 and every dollar to Iran is a factory not
   built at home. Programs persist with zero upkeep; dropping one is
   the decision.
3. **Fast verbs are espionage operations.** Coups and election pushes
   consume the existing intel network (strength gate, strength cost,
   the `op-resolve` blown roll), are gated on *visible local facts*
   (stability below a gate, who equips the army, an open election
   window, the tension band) rather than cash, print an outcome ladder
   in estimative words with one legend, and carry a backfire branch
   that moves the target *against* the sponsor. History: 64 US covert
   attempts 1947-89, 39% installed the preferred side; Syria 1957,
   Indonesia 1958 and Cuba 1961 all moved alignment the wrong way.
   Even a clean coup pays tension and legitimacy.
4. **Commitment depth is hysteresis plus locks, not a second
   variable.** Entering a bloc band needs |position| ≥ 300 sustained
   across a month boundary, leaving needs < 150; treaty and satellite
   status are `locked_until` timers set only by explicit acts (1950
   seed table, accession events, settlement clauses, a CRUSH option)
   and cost tension and legitimacy to break. Reversal within twelve
   months of a band change runs at half rate. This kills Vic2's sphere
   ping-pong and Terra Invicta's launch-week whack-a-mole.
5. **Scoring is regional Presence / Domination / Control over a
   sourced battleground set**, computed monthly, printed as the
   reserved INFLUENCE STANDINGS newspaper page, and frozen by a
   checkpoint resource at 1955/1960/1965. Non-Aligned counts as
   *denied* for the bloc that does not hold it, which is the only
   reason both blocs care about keeping a state out of the other's
   column. Battlegrounds never leak the script: a 1950-sourced base
   set plus open contested windows, never era-scheduled additions.
6. **Every Independence opens a contested window.** The newborn seeds
   inside the NonAligned band leaning toward its baseline pole,
   `contested_until` = birth + 24 months, program rates doubled and
   hysteresis suspended inside the window; the window closes on its
   date, the first lock, or a coup, with a paper headline. The ~10
   chains that assume the historical outcome are retargeted to shoves
   and band-gated variants. (History: the bidding window was real and
   12-18 months long; Moscow concentrated on Guinea, Ghana, Mali.)
7. **Legitimacy is the one world-opinion currency; no second ledger.**
   It is already written by the Hungary decision and France's NATO
   exit and spent as a treaty gate. CRUSH re-locks the satellite,
   debits legitimacy, spikes tension, increments a never-decaying
   `crushed_count`, and while legitimacy sits below threshold halves
   the crusher's program rates in every contested country — the bill
   lands on what the crusher competes for, not on a British election
   Moscow was never going to win.
8. **Non-Aligned is a field with a score in v1, an actor in v1.5.** The
   passive middle emerges from band arithmetic (denial is cheaper than
   acquisition); Bandung (Apr 1955) and Belgrade (Sep 1961) widen a
   pull toward 0 in unlocked states near NAM champions and grant the
   hosts legitimacy; NonAligned resolutions carry the highest inertia.
   Nehru, Nasser and Tito get verbs (solicit bids, host a conference,
   expel advisers) with the auction. Never a third AI with its own win
   condition — the one game that tried (QMG: Cold War) needed a
   bespoke victory to make it work.
9. **Legibility follows the house rules**: one verdict sentence per
   country ("EGYPT: CONTESTED, LEANING EAST (EST.), MOSCOW ACTIVE, NO
   TREATY"); bands and Kent's estimative words, never permille or
   percentages; foreign positions render through political-domain
   penetration as a bracket with an AS-OF stamp; every band crossing
   and rival spend is a wire line with a named cause; own-bloc rows
   are a filter chip in the same ledger.
10. **Decision count is a designed budget**: 3 standing slots + 1
    active-op slot for a 1950 superpower (4+2 by 1955 via dated
    unlock events), 2+1 for UK/France, 1+0 for other majors, 0 for
    minors; a ~12-row battleground triage list as the default view;
    ≤ 6 wire lines a month; elections as dated valves only for the
    ~15 battleground democracies; pull decisions arrive only as offers
    with deadlines. Historical cadence was a handful of large
    commitments a year.
11. **AI participation in v1 is a published, deterministic, attributed
    allocator over overt verbs only** (aid/presence) for USA and USSR,
    reading a month-start snapshot so sponsor order is never
    load-bearing, printed monthly as "MOSCOW'S CHEQUEBOOK". The
    "rival active here ×2" bait multiplier is dropped. Coups and
    election ops stay timeline events plus player-only ops; AI covert
    is the first item in the return queue, named, not assumed.
12. **Ship the calibration harness with the pillar**: extend the
    20-year hands-off test with alignment-band snapshots at 1955/1960/
    1965 for ~30 sourced anchor countries, a sourced per-country
    stability pass (73 of 86 sovereigns sit at a flat 60 today, so
    every country is the same coup target), and a 1950 treaty/lock
    seed table — through the generator pipeline, since `generated.ron`
    is never hand-edited.

## What the skeptics killed

- **Living-standard drift and neutral gravity** (both fatal): the US
  standard of living exceeds the Soviet one for the entire window, so
  every unlocked country drifts West free forever; neutral gravity
  decays ~100 baseline-aligned countries to NonAligned before Korea.
  Deleted from v1; the systems-competition term returns as a
  checkpoint score line once the harness can show it does not repaint
  the hands-off Third World.
- **Aid as a per-country cash verb and 1.5-2 ops/month** (fatal): the
  bank reopened by the back door, and five times Twilight Struggle's
  ops density. Aid goes in a slot; one op slot with three-month prep
  caps a superpower near history's rate.
- **Three-faction triads** (serious): four analysts converged on the
  word and shipped four different triads with no seeds for 260
  countries — a drift simulator of 700+ integers. v1 stores one
  position, dynamic stability, and an `army_patron` flag; the triad
  ships in v1.5 only behind a written decision test.
- **Adjacency realignment and reach rules**: a regional snowball
  without TS's card hand to brake it, and ahistorical for the decade
  (Egypt 1955, Indonesia 1956, Guinea 1959, Cuba 1960 were all
  non-contiguous). Dropped.
- **Crush via occupation zones**: zones are war-derived and holder-
  relative; a peacetime crush has no key. The mirror is priced on the
  existing event options instead.
- **UNGA as a second scoring surface**: tautological until votes are
  issue-specific and vote-buying is a verb. v2.
- **The briefing recommendation line**: a correct one plays the game,
  an incorrect one is noise. The card keeps facts only.

## Historical calibration (from the historian)

| Date | Tool | Actor→target | Cost | Time to effect | Result |
|---|---|---|---|---|---|
| Apr 1948 | Election funding + overt aid threat | US→Italy | ~$1M covert | 4 months | DC 48.5%; the template |
| Jun 1948 | Bloc discipline | USSR→Yugoslavia | expulsion | immediate | Lost to non-alignment; US aid from 1950 |
| Feb 1950 | Treaty + credit | USSR→PRC | $300M | immediate | Consolidated; split anyway by 1960 |
| Aug 1953 | Coup | US/UK→Iran | $285K of $1M | 5 months | Success on the second attempt |
| Jun 1954 | Coup by manufactured belief | US→Guatemala | ~$2.7M | 10 months | Success; 36-year civil war |
| Sep 1955 | Arms deal | USSR→Egypt | $80-250M in cotton | 6-12 months | Access, not loyalty; expelled 1972 |
| Jul 1956 | Aid withdrawal | US→Egypt (Aswan) | — | 7 days | Canal nationalized; Suez |
| Nov 1956 | Crackdown | USSR→Hungary | military | 12 days | Bloc held; PCI/PCF bled, decade of ammunition |
| Aug 1957 | Officer bribery | US→Syria | small | months | Failed, exposed; Syria into the UAR |
| May 1958 | Rebel support | US→Indonesia | ~$10M + B-26s | 4 months | Failed; Sukarno bought Soviet arms |
| 1958-61 | Recognition + credit | USSR→Guinea | ~$35M | 10 months | Gained, then lost Dec 1961 |
| Sep 1960 | Coup + plot | US/BEL→Congo | ~$100K | 2-4 months | Western Congo; Lumumba a martyr |

Levin's 117 electoral interventions 1946-2000 average +3 points for
the favoured side; overt help outperformed covert. The five mechanisms
that mattered, in order: security-elite capture (whoever equips the
army owns the coup option), windows (money outside one is stored, not
spent), the auction (a rival's offer triggers yours; a withdrawal is a
crisis), nationalist backlash (attribution, not cost, was the
deterrent), and factions as the store of influence.

## The recommended v1 slice

**State**: `Influence` resource — per country `position: i16`,
`baseline: i16`, `locked_until`, `contested_until`, `last_band_change`,
`army_patron`, `crushed_count`; monthly `Standings`; a digest-folded
`Checkpoints` resource. Positions seed from existing `CountryDef`
alignment (±450 aligned, ±700 and locked for treaty/satellite from the
seed table, 0 NonAligned). The only passive motion: program-bought
lean decays 5/month toward `baseline` when no program is active; after
Bandung, unlocked neighbours of NAM champions pull 5/month toward 0.
**Verbs**: standing programs in slots (AID from the pool with an
announcement step, a monthly flow and a WITHDRAW that reverses and adds
tension; PRESENCE near-free, halved in closed regimes); active ops
through `intel.rs` (ELECTION PUSH offered six months before a
battleground democracy's sourced election, ±60 on a seeded roll;
SPONSOR COUP gated on stability < 50, army patron, no lock, tension
band, three-month prep, five-rung ladder in Kent words). **Surfaces**:
INFLUENCE map mode; one politics panel with a ~12-row battleground
triage list, a commitments tab and a one-page political dossier per
country; INFLUENCE STANDINGS and THE COLONIAL QUESTION pages in the
paper; ≤ 6 wire lines a month. **AI**: one published monthly allocator
per bloc leader over overt verbs. **Decolonization**: Independence
opens a 24-month contested window; outcome-assuming chains retargeted.
**Mirror**: MY BLOC is a filter chip; the uprising events gain
CRUSH/TOLERATE pricing. **Feeds**: every `alignment_of` consumer
unchanged; ClientState/Neutralization clauses get teeth; released
occupation zones flow into position through the holder's bloc frame;
`Domain::Political` gets its first consumer; victory reads
`Checkpoints`.

## Open questions, answered

- **Non-Aligned**: an active attractor in the design, shipped as a
  field with a score in v1 and as an actor (the auction) in v1.5.
  Never a third AI player.
- **Granularity**: per-country in v1 (position + dynamic stability +
  `army_patron`) — enough to make aid, presence, election push and
  coup distinct verbs. A fixed Government/Army/Street triad in v1.5,
  only when a decision test shows a decision the axis cannot express.
  Never a per-country faction list.

## Deferred, in intended return order

Attribution three-state machine and a crisis pretext API (shared with
espionage v1.5); AI covert participation (abstracted election pushes
and coups by the same allocator); the Government/Army/Street triad
with a regime valve; the Aswan auction (aid bids as placed projects, a
recipient decision rule, NAM actor verbs); the metropole as a distinct
bidder and the PRC as a third pole after the split; the extended
election calendar and cadre pipelines; UNGA as a second scoring
surface; the living-standard term as a checkpoint score line.

Next per convention: advance `docs/design/systems/influence.md` from
sketch to designed via the design-doc skill, citing this readup.
