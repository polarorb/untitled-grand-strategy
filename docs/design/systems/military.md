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

## Limited war & armistice

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

## Sequencing (agreed with the research)

1. **Korea slice**: event system, starter formations from archetypes
   with fixed 1950 equipment, movement + combat core, theater directive
   v1, the June 25 invasion.
2. Equipment designer + production-line integration.
3. Manpower policy, reserves, readiness states.
4. Armistice negotiation layer; deniable contingents.
5. Full logistics solver in theaters.
