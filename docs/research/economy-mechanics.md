# Economy mechanics — research readup

Researched 2026-08-25 by an 8-analyst swarm across the genre: HoI4,
Victoria 3, Victoria 2 + EU4, Supreme Ruler (esp. Cold War), Workers &
Resources: Soviet Republic + Factorio, the Cold War niche (Terminal
Conflict, Crisis in the Kremlin, Ostalgie, Mao's Legacy, Suzerain,
Twilight Struggle), Civilization IV–VI + Old World/Millennia, and
Stellaris + economy-design literature (Machinations, Koster, GDC).
Full structured reports: [economy-mechanics-raw.json](economy-mechanics-raw.json).

The striking result: eight independent analysts converged on the same
principles. Where they all agree, we should treat it as settled.

## The ten convergent principles

1. **Stock cost + permanent flow cost, everywhere.** HoI4's civ-factory
   snowball and Stellaris' anti-snowball upkeep teach the same lesson from
   opposite directions: every building must cost capital to build AND burn
   labor/power/maintenance forever. Growth becomes a portfolio choice, not
   compound interest, and no artificial caps are needed.

2. **Never one self-purchasing currency.** HoI4 (civs build civs) and
   Victoria 3 (construction sectors are buildings) both collapsed into a
   solved build-more-builders loop. Construction throughput must be gated
   by things it cannot directly buy: cement/steel output, skilled labor,
   electricity — so the binding constraint migrates across the campaign.

3. **Population is the universal denominator — and ours is real.** Every
   output is people × workplace × method (Stellaris' deepest idea; its 4.0
   workforce retreat proves the implementation: continuous cohorts, never
   discrete pop objects). With real 1950 census data per province, real
   demography (SoL-driven birth/death à la Victoria 3) replaces artificial
   growth brakes and makes the baby boom, the Third World explosion, and
   Soviet stagnation emerge from play.

4. **Labor, not money, is the era's binding constraint.** The Soviet
   growth story is rural reserves moving into factories, then exhaustion —
   a built-in planned-economy late-game brake needing no scripting
   (Supreme Ruler is the rare game that models this). Urbanization rate,
   gated by housing and food logistics, IS the industrialization dial.

5. **Electricity is a regional, non-storable flow — never money and never
   global.** Stellaris conflated power with currency (so blackouts can't
   exist); Supreme Ruler made it globally tradeable (so grids don't
   exist); Victoria 3 made it per-state micro-hell. Consensus: named grid
   regions, generation vs demand, deficit softly throttles industry
   (Civ VI's ~50% soft degradation, never a hard stop). Enrichment plants
   drawing whole percents of national power weld the bomb to the grid.

6. **The bomb is a supply chain with an intelligence footprint.** Every
   analyst independently: uranium geography (Congo, Joachimsthal,
   Colorado) → enrichment/reactor-plutonium facilities (huge, slow,
   power-hungry) → fissile stockpile in kg → device → detectable test →
   weaponization → delivery race. Each stage burns shared national pools
   (construction, scientists, electricity) and each is partially visible
   to rival espionage — the "when will they get it?" drama emerges from
   the economy. Never a research slider; and the game must stay
   interesting AFTER both sides have it (Civ's nuclear age is an
   epilogue; ours is the whole game).

7. **Planned vs market = quantities vs parameters.** The precise
   formulation, reached by all: one shared stock-flow simulation, two
   verb sets. The **planner sets quantities** (five-year-plan quotas,
   material balances) and the sim solves for shortages — failure modes:
   misallocation, queues, famine, black markets, and falsified fulfillment
   reports that corrupt the player's own dashboard. The **market player
   sets parameters** (taxes, rates, tariffs, procurement contracts) and
   autonomous firms solve for quantities — failure modes: recessions,
   unemployment, inequality feeding the influence war. Supreme Ruler
   proves "communism = capitalism with modifiers" is a dead end; Victoria
   3's command economy is the confession of failure to study.

8. **Locality makes economic warfare playable.** Empire-wide fungible
   pools (Stellaris) make blockades meaningless. Resources (~6–10:
   oil, coal, steel, grain, uranium, rubber...) move along severable
   routes; stockpiles are first-class objects ("how many months of oil
   does Japan have" is a number, and an intelligence target). Two trade
   systems: floating world prices for the dollar bloc (oil shocks
   emerge), negotiated Comecon barter for the socialist bloc, COCOM
   export controls as a weapon, the non-aligned world as the contested
   membrane.

9. **Dual currency for the East.** Soft domestic rubles (always
   sufficient, buys only domestic capacity) vs scarce hard currency
   (buys Western tech and grain) — the Kremlingames titles prove this one
   cheap mechanic delivers more systems-competition than any production
   chain. Gold, oil, and arms exports become strategic map assets;
   Western credit becomes an economic-warfare lever.

10. **Attention is the scaled resource; presentation is the loop.** At
    4,594 provinces nobody places factories (even Gosplan didn't).
    Aggregate decisions to ~one construction decision per game-month;
    named Great Projects (Volga-Don, Aswan, Interstate) with timelines,
    overrun risk, and influence payouts carry the memory; generic
    capacity handles volume. Chunky discrete allocations beat sliders.
    Every emergent outcome needs a "why" inspector (Vic2's opacity =
    wiki homework), except where degraded information is the point: the
    planner's dashboard shows plan data, not truth.

## Anti-patterns (unanimous)

- Consumer goods as a dead-weight tax (HoI4) — households must actually
  demand goods; relative deprivation vs the other bloc (Ostalgie's
  Westalgia + Civ VI loyalty) turns living standards into the influence
  battleground.
- Stacking +5% modifiers as progression — mudflation; use step-change
  production methods so 1980 numbers still mean something.
- Abstract mana bought outside the simulation (EU4).
- Fixed-price barter world trade (HoI4).
- Free policy-regime switching — transitions must cost years (this also
  gifts us the 1989 collapse endgame).
- Per-province build queues, vehicles, substations, fields (W&R at
  country scale: "keep every dependency, delete every entity").

## What this means for UGS (build order)

1. **Demography first** (cohorts per province: rural/urban/educated;
   SoL-driven vital rates) — it's the denominator of everything else.
2. **Resources + regional grids** (short commodity list, flow balances at
   region level, electricity as regional throttle).
3. **Construction capacity + the two interfaces** (plan quotas vs
   budget/parameters) — the pillar, built as two verb sets from day one.
4. **Agriculture & procurement** (subsistence as default land use;
   collectivization / land reform / plantation-export as rival exits;
   quota slider with famine on one side and budget cost on the other).
5. **The nuclear chain** on top of grids + construction + scientists,
   feeding espionage and brinkmanship.

Full per-game detail, strengths/weaknesses, and numbers are in the raw
JSON. The design consequences are being folded into
[design/systems/economy.md](../design/systems/economy.md).
