# Intelligence & Covert Ops

Status: designed (research locked; implement v1 first, phases below)
Pillar: 3 (primary); couples to all others
Research: [espionage](../../research/espionage.md) (5-analyst swarm)

North star: **what your rival believes matters as much as what is true;
deniability is a currency.** The nuclear pillar already computes
deterrence from biased estimates — this layer moves those beliefs, on
both sides, and prices the cost of being caught.

## Architecture at a glance

Two products, one resource, deterministic throughout:

1. **Knowing** — a four-domain penetration score per (viewer, subject)
   that replaces every hardcoded fuzz width and opacity bias already in
   the sim with a live, earned value.
2. **Doing** — a single abstract network per (owner, target) that both
   collects intelligence and, when spent, runs operations; every op
   carries a legible exposure clock and, if blown, a three-state
   attribution machine that feeds the crisis system's pretext demand.
3. **The price** — counterintelligence, deniability, spy trials, and
   the taboo-priced operations, all routed through the tension meter
   and the existing teletype/command machinery.

## 1. Knowing — the intelligence economy

`IntelState`: `BTreeMap<(CountryTag, CountryTag), DomainPenetration>`
where `DomainPenetration { nuclear, military, economic, political }`,
each 0-1000 permille. Four domains because four consumers already
exist, 1:1:

| Domain | Consumer (already in code) | Effect |
|---|---|---|
| nuclear | `deterrence.rs` opacity bias | viewer knowledge = max(subject exposure, penetration.nuclear); shrinks the bomber-gap bias |
| military | war-UI fuzz widths | `width = BASE * (1000 - pen)/1000` (floor so never exact) |
| economic | `planning.rs` reported vs actual | rival-visible output = lerp(reported, actual, pen/1000) |
| political | `crisis.rs` Resolve band | narrows the enemy-resolve estimate |

**Growth**: monthly (`new_month`), asymptotic to the best active
source's cap — `intel += (cap - intel) * rate/1000 + 1` (the +1
guarantees integer termination). **Decay** toward the remaining floor
when sources lapse (25-40 permille/month). Two separable consumables:
WIDTH shrinks with any intel; BIAS shrinks only with verification-class
sources — so parade deception survives until an overflight or satellite
sees through it, and the missile-gap window (May-Aug 1960) reproduces
for free.

**Sources** (each a cap + rate + risk shape):
- Embassy/attaché — automatic with relations, cap ~250, all domains,
  the floor that makes intel 0-9 a pariah-only state.
- Agent networks — the workhorse; sole access to economic truth and
  political resolve; caps reduced by target counterintel; roll-up risk.
- SIGINT/decrypts (v2) — standing program, retroactive product,
  unusable in public without burning the source, killed by cipher
  changes.
- Overflights (v2, 1956+) — per-mission certainty purchases;
  verification-class (kills bias); shootdown risk rising with SAM tech.
- Satellites (v2, 1960+) — expensive, early failures, then risk-free;
  ends strategic fog permanently, shifting the late game onto economics
  and resolve.

## 2. Doing — networks and operations

One abstract **network** per (owner, target): `strength: 0-100`, funded
0-3 via `SetNetworkFunding`, growing to a funding-set ceiling slowed by
target counterintel and regime openness. No decay from inaction, no
upkeep micro, no agent roster. Collection is passive; operations
**spend and risk** the same network (the OSO/OPC fratricide as one
resource — a network strong enough to sabotage is a network delivering
estimates you'll lose).

**Operation lifecycle — three player decisions, the rest auto-resolves:**
1. AUTHORIZE — type, target, posture dial (cautious/standard/aggressive:
   trades prep time and evidence-if-blown against odds). Presented with
   the full **outcome ladder** — named world-states with period
   likelihood words, never a success %.
2. PREPARATION — weeks/months of sim time; exposure can breach; clean
   abort available.
3. (auto) EXECUTION — resolves from a forked stream, no mid-op micro.
4. AFTERMATH stance — only if blown: cover story / disavow / trade,
   chosen before the enemy's evidence is fully known.

**Operation catalog** (v1 bold, rest phased): **nuclear sabotage**
(knock a facility level / apply a program malus), **steal designs**
(the Fuchs effect: −duration on the rival's design work + pierce its
opacity), *émigré network insertion, coup sponsorship (capstone — wins
via manufactured belief, belongs to influence), clandestine radio &
balloon leaflets (standing programs), cable tap, defector inducement,
executive action (taboo-priced, delayed-fuse attribution that can fire
years later when the operator defects), party funding & arms pipelines
(set the volunteer-formation deniability already in the war slice).*

## 3. The price

- **Exposure clock** per network/op: every use adds a known amount;
  breaches are discrete named events ("courier detained in Vienna");
  blowback fires at inspectable thresholds plus seeded timing jitter.
- **Attribution**: UNATTRIBUTED → SUSPECTED → PROVEN, moved by named
  evidence objects. Only PROVEN is a crisis pretext; unacknowledged
  ops escalate less (deniability as an escalation damper).
- **Counterintel**: structural floor by regime openness (closed ~300)
  + funded component from the same agency budget; monthly sweep rolls;
  a catch fires a **spy-trial** choice event — show trial (+domestic,
  +tension, rival embarrassed) / quiet expulsion (banks a swap) / turn
  the agent (feeds deception, v1.5).
- **Deniability** counter: each blown op spends it; at low deniability
  the next blown op's tension cost multiplies (the U-2 problem);
  scapegoat command rebuilds it at domestic cost (v1.5).
- **Deception**: per-domain subject posture (replaces the current
  `SetParadeDeception`); seen through above a penetration threshold
  ("SOURCES CONTRADICT OFFICIAL FIGURES"), permanently flagging a known
  deceiver.
- **Radio playback**: a rolled-up network the enemy chooses to *watch*
  keeps showing product and feeds poisoned estimates until counterintel
  catches it — the intel mirror of reported-vs-actual (v1.5).

## Presentation

An Intelligence panel (`UGS_PANEL=intel`) of dossier cards, one per
rival per domain: a period coverage grade (NEGLIGIBLE / LIMITED /
PARTIAL / EXTENSIVE), the current estimate with its band, an AS-OF
date, and an NIE-style provenance line — plus your own counter-exposure.
Never raw permille. Three textual registers for op product, all
deadpan: PUBLIC WIRE (what the world sees; you read your own blown op
as a news item), INTERNAL CABLE (numbered paragraphs, hedged —
"ASSESS CAPTURE PROBABLE"), ENEMY BROADCAST (show-trial coverage
dripping over weeks). The war-UI and deterrence displays keep their
look; their widths just go live.

## Determinism & scheduling

Integer permille in BTreeMaps keyed by CountryTag pairs; passive
accrual/decay on `SimClock::new_month`; discrete incidents from labeled
forks (`b"ci-sweep"`, `b"op-resolve"`, `b"defector"`, `b"crypt-burn"`)
iterated in sorted key order; every decision a `SimCommand` in the
replay log; event choices through `ResolveEvent`. Systems run in
`TickSet::Politics` after Commands so a sabotage lands the same month.
Display fuzz stays presentation-side. Tests: same-seed bit-identical
with espionage active; penetration monotonically narrows the deterrence
bias; month-rollover and 1952 leap-year cadence.

## Sequencing

1. **v1** — IntelState + the four one-line couplings (land before any
   new content, so every existing system visibly improves); networks +
   funding commands; counterintel + sweeps + spy trials; nuclear
   sabotage & steal-designs with blown-op → tension + pretext +
   deniability; minimal defectors; the Intelligence panel; the fixed-
   date public-shock events (Fuchs, Burgess/Maclean, Rosenberg, Petrov).
2. **v1.5** — mole hunts, turned agents / radio playback, swaps,
   scapegoats.
3. **v2** — overflights (shootdown crisis loop), Venona-style decrypts
   with source-burning. Coups/election ops arrive with the influence
   pillar and consume this layer.

## Resolved questions (from the sketch)

- Agent granularity: **abstract networks + event-borne named
  instruments** — no roster, confirmed by every genre precedent.
- Feeding the AI deception: deception adds bias only while the viewer's
  penetration is below threshold; above it, seen through and flagged.
- How much wrongness is fun: bounded by the decile reveal table — the
  player always knows their coverage grade and can act to improve it,
  so wrongness is a state they chose to tolerate, not random noise.
