# Military

Status: architecture locked by research (detailed per-subsystem specs
pending; Korea slice first)
Pillar: serves 1 primarily — war is the escalation pillar's expression
Research: [military-mechanics](../../research/military-mechanics.md)
(8-analyst swarm, unanimous architecture)

## The shape

**Design deep, command high.** Players author their military at two
levels — equipment generations (the deep layer) and doctrine-archetype
formations (the light layer) — then command at theater altitude while
the AI executes provinces. Most conflict is limited war under a priced
escalation ceiling; wars end at the line of control.

## Equipment design (the player's deep layer)

- Design *generations* per domain (tank, rifle/APC kit, fighter, bomber,
  escort, submarine...): 5–6 opposed soft stats (firepower, protection,
  mobility, reliability, cost, logistics footprint = fuel + ammo draw)
  chosen from discrete named options gated by the calendar via shared
  component ladders (gun, armor, engine, FCS, missile, radar).
- Hard constraints price everything: steel/oil/military_stock draw,
  factory line time, multi-year development lead, later treaty caps.
- Production lines carry HoI4-style efficiency; new generations mean a
  retooling valley — design churn costs real industry (the spine already
  built in the planning system).
- Mid-life refits; obsolescence cascade policy: front-line → reserves →
  exports/proxies → war stock. Old marks arm the influence war.
- Designs are espionage objects both ways: enemy designs known only as
  estimates until stolen/captured/observed in combat; after-action
  reports are live-fire R&D.
- Competent doctrine-flavored auto-designs ship so the workshop is
  opt-in mastery.

## Formations (the light layer)

Curated doctrine-era archetypes (per nation family and era: triangular /
Pentomic / ROAD; rifle / motor-rifle; airborne, marine, security), whose
equipment slots take player designs and whose manpower policy (conscript
term, training intensity, reserve categories) the player sets nationally.
Edit the archetype once — the force converts on a rate-limited schedule,
in place, keeping cadre (conversion downtime is itself an escalation-
window consideration). No free-form width puzzle.

## Combat resolution

HoI4's proven skeleton, de-spreadsheeted: persistent battle entities in
contested provinces, hourly resolution, **cohesion** (fast recovery,
decides battles) vs **strength** (slow, consumes economy). Smooth curves
— no threshold cliffs, no non-additive aggregation, no chaff-rewarding
targeting. Frontage from real province edge geometry, visible on map.
Randomness only in the 12–24h tactics draw. Encirclement via supply
dominance + retreat-path surrender + exploitation movement. Theater
fatigue/readiness governor: offensives prepare, pulse, culminate.
Weather/supply attrition rivals combat losses (Chosin).

## Command: theaters, postures, ROE

Player-created persistent theaters with player-drawn boundaries; per
theater: posture (defend/probe/offensive) × axis/objectives × reserve
commitment × **ROE bound to the escalation system** (pursuit limits,
bombing depth, sanctuary borders — "may not cross the Yalu" is a
setting, and violating it is an escalation incident). No per-unit micro
layer exists. Battle results fully auditable.

**v1 spec: [military-command](military-command.md) (implemented)** —
theaters, frontline distribution, active/reserve readiness, and force
generation from `military_stock`, with an interim commitment-tension
brake until the priced escalation ceiling lands.

## Limited war & armistice

**Successor spec: [war-termination](war-termination.md) (implemented v1)** —
war aims, the settlement table, occupation zones, and outcome objects
(Treaty / FrozenConflict / ImposeSettlement) extend and supersede the
v1 armistice below.


- Intervention levels, volunteer/proxy contingents with **cover levels**
  (deniability stripped by enemy espionage; proven ≫ suspected).
- Tripwire risks (Chinese entry) shown as espionage-quality estimates.
- **Armistice as negotiation state**: line of control vs war objectives,
  casualties (drawn from real cohorts — geographic, political), patron
  pressure, weariness; talks-while-fighting makes provinces bargaining
  chips; outcomes freeze into DMZs and standing crises.

## Logistics

Lazy evaluation: full flow-solving only in active theaters; peacetime
world rides the economic regions. Supply draws the same rail/oil numbers
as the civilian economy. Three visible states (Supplied / Strained /
Cut) with a path overlay. Ports are the amphibious mechanic. Logistics
decisions are investments and priorities, never convoy micro.

## Manpower & legibility (implemented v1)

Armies are drawn from the simulated population — never spawned free.
Each country seeds a reserve pool at 1.5% of its real population;
belligerents mobilize a further 0.2%/month; formations resting on
friendly soil reinforce at 15 strength/day paid from the pool (one
strength point = ten men). Casualties, fielded strength, reserve, and
population reconcile in the war room's pipeline strip.

The war itself is presented per the
[war legibility research](../../research/war-legibility.md): live
`BattleView` snapshots each combat hour (sides, men, cohesion, hourly
attrition, modifiers), pulsing battle markers, a battle inspector with
an inline signed modifier ledger, break-time projection and a one-line
"why you are losing" diagnosis, dual strength/cohesion bars on unit
counters, a wire-service war ticker, and battle win/loss tallies.
Enemy figures display as monthly-resampled two-significant-figure
ranges (display-side hash, sim RNG untouched); own losses are exact.

Divisions have identity: each is raised from a home province and named
for it ("3RD SEOUL INFANTRY" — expeditionary forces raise from their
nation's most populous province), and its war dead come off that
province's actual population, rural cohort first. Moving divisions
trail arrows on the map. Each war carries a decomposed momentum score
(ground / exchange / battles, every term visible) with a generated
assessment line.

## Sequencing (agreed with the research)

1. **Korea slice**: event system, starter formations from archetypes
   with fixed 1950 equipment, movement + combat core, theater directive
   v1, the June 25 invasion. *Done, including armistice at the line of
   control, manpower pools, and the war-legibility layer.*
1b. **Command layer + force generation**: theaters, readiness,
   raising divisions from `military_stock` — see
   [military-command](military-command.md). *Done.*
2. Equipment designer + production-line integration.
3. Manpower policy (conscription laws), readiness states. *(Home
   provinces with local casualty debits: done.)*
4. Intel as a real per-country stat driving estimate width; sighting
   staleness; deniable contingents.
5. Full logistics solver in theaters.
