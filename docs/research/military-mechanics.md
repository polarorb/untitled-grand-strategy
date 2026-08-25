# Military mechanics — research readup

Researched 2026-08-25 by an 8-analyst swarm: HoI4's designers and combat
math, the deep end (Rule the Waves, Aurora 4X, SMAC, Stellaris), Wargame/
WARNO deck building and doctrine, combat-resolution models (EU4, AGEOD,
Unity of Command, hex CRTs), front management (HoI4 planner, Victoria 3
fronts, HoI3 OOBs, Supreme Ruler), limited war (Victory Games' Korean
War 1986, Next War: Korea, Twilight Struggle, Vietnam '65, Fire in the
Lake), logistics (NSB, UoC, War in the East 2), and an integration
synthesis. Raw: [military-mechanics-raw.json](military-mechanics-raw.json).

Design brief going in: **players design their own units and fight wars**,
at a game whose thesis is that total war is the failure state. The
analysts converged hard. The architecture:

## 1. Two layers, asymmetric depth

**Equipment design is the deep, player-authored layer** — Rule the Waves
is the revered model: design the tank / interceptor / SSBN *generation*
(~1–2 big decisions per domain per decade) with **5–6 opposed soft
stats** (firepower, protection, mobility, cost, reliability, logistics
footprint) under **hard external constraints** — budget, steel/oil draw,
factory time, and later treaty caps. Multi-year lead times, decades of
service life, refits. Discrete named choices, never sliders: "sliders
get solved into a formula, modules get solved into stories."

**The formation layer is template-lite** — adopt HoI4's one genius move
(design once, army follows, edit-once-upgrade-all) but as a **curated
menu of doctrine-era archetypes** (US: triangular → Pentomic '57 → ROAD
'63; Soviet: rifle → motor-rifle; airborne, marine), whose equipment
slots take your designs and whose manpower mix you tune. No free-form
combat-width puzzle: every HoI4 degenerate meta (40-width, space
marines) traces to hidden cliffs, non-additive aggregation, or
deterministic targeting — all avoidable at the spec stage.

## 2. Combat: HoI4's skeleton, de-spreadsheeted

The only proven fit for RTwP + hourly ticks + thousands of provinces:
persistent per-province battle entities, computation only for active
battles, and the **two-track damage model** — fast-recovering *cohesion*
decides battles, slow-recovering *strength* consumes the economy.
Cohesion breaking before annihilation IS limited war. Divergences, all
unanimous: smooth curves instead of threshold cliffs (+1 stat ≈ +1
outcome, always), additive aggregation, frontage derived from real
province geometry and shown on the map, randomness only at the readable
"tactics draw" layer. Encirclement needs three shipped mechanisms:
supply dominance (cut units bleed cohesion daily), retreat-path
surrender rules, and exploitation paid in movement. A fatigue/readiness
tempo governor prevents WW1 grinding — and residual stalemate is a
*feature*: grinding fronts are the pressure that tempts players up the
escalation ladder.

## 3. Command altitude: theaters and directives

The genre's two catastrophic failures (HoI4 front-shuffling, Victoria
3's vanishing fronts) share one root cause: forces assigned to emergent
objects derived from live map geometry. Rule zero: **players assign
forces to player-created, persistent theaters**, set posture × ROE ×
objectives, and the AI executes province-level positioning with
hysteresis. Crucially (Supreme Ruler's lesson): no per-unit micro
fallback at all — if micro exists, delegation dies. 3–6 meaningful
spatial decisions per theater ("hold Pusan, prioritize the Naktong,
land at Incheon") with auditable battle results.

## 4. Limited war is the game

- **Wars end at the line of control**, not capitulation: armistice as a
  first-class negotiation state (front position vs objectives,
  casualties, patron pressure, weariness); fighting during talks turns
  provinces into bargaining chips; DMZs persist as world-state.
- **The escalation ceiling lives inside the war and is priced, not
  scripted** — Victory Games' Korean War (1986) solved this: choose
  intervention level, mobilization, ROE (bombing zones, atomic release)
  and pay tension per turn for aggressive settings. Tripwires (Chinese
  entry when you approach the Yalu) shown only as espionage-quality
  *estimates* of the true risk.
- **Deniability is a stat**: proxy contingents (volunteers, pilots)
  carry a cover level that enemy espionage strips; *proven* involvement
  escalates, *suspected* only moves opinion.
- War weariness emerges from cohort casualties (geographic, political),
  with escalating marginal cost per reinforcement; US election cycles
  as natural clocks, Soviet strain through elite politics.

## 5. Logistics: lazy, economy-native, legible

Full supply-flow solving **only inside active war theaters**; the
peaceful 95% of the world piggybacks on the economic regions — the big
structural difference from WW2 games. Military supply draws on the SAME
rail capacity and oil balances as the civilian economy (mobilization
crowds out freight: guns-vs-butter for free; interdiction and sabotage
become supply weapons with no new systems). Designs carry exactly two
supply stats — fuel draw and ammo draw — so Korea's asymmetry (US
mechanized/port-tethered vs Chinese light/interdiction-immune) falls
out of the designer. Ports are the amphibious mechanic (Incheon as a
systems-native masterstroke). Player-facing: three states — Supplied /
Strained / Cut — with a path overlay. Zero recurring micro.

## 6. Manpower quality is a second capital stock

Conscript intake from the demographic cohorts (educated → officers and
technical branches, *competing with the civilian economy for the same
people*), a decaying trained-reserve pool (Soviet Category A/B/C
mobilization tiers ready-made), an officer corps built slowly and
destroyed fast (purges as a real authoritarian lever). Policy-level
only — never individuals (WitP:AE's tedium warning).

## 7. The Cold War payoffs no WW2 game has

Designs are **influence and espionage objects**: obsolescence cascades
automatically (front-line → reserves → arms exports → proxies — arming
the world with your old T-54s is a mechanic, not flavor); stolen
blueprints and combat-captured equipment reveal design stats; planting
false specs is counterintelligence; limited wars are **live-fire R&D**
("your T-54's frontal armor was defeated by 105mm APDS at 1,500m" —
MiG Alley as a proving ground). Counter-design pressure from the rival
superpower is the systemic anti-meta: every exploit is a temporary edge
that espionage-informed procurement answers. Readiness states
(active / category-B / mothballed) are visible mobilization signals
feeding brinkmanship.

## Build order implication

Event system + minimal formations/movement/combat for Korea first
(archetypes with fixed starter equipment); the equipment designer
arrives as its own major system once combat exists to give designs
meaning; manpower policy and logistics deepen in parallel with the
theater layer.
