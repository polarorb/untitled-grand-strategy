# Economic Systems Competition

Status: sketch → architecture locked by research
(see [research/economy-mechanics.md](../../research/economy-mechanics.md);
detailed per-subsystem specs still needed before implementation)
Pillar: 4

## Thesis

One shared stock-flow simulation of the world economy; **two asymmetric
player interfaces**. The planned economy sets **quantities** and the sim
solves for shortages. The market economy sets **parameters** and
autonomous firms solve for quantities. Different verbs, different failure
modes, different cadence of attention — if a new player can't articulate
why the two blocs feel different within the first hour, this pillar has
failed.

## The shared substrate

- **Demography (the denominator).** Per-province continuous cohorts
  (rural / urban worker / educated, coarse age bands) seeded from our real
  1950 census data. Birth/death rates driven by standard of living
  (Victoria 3's curve, calibrated to reproduce 1950–1990 historical
  trajectories as the no-intervention baseline). Urbanization — gated by
  housing and food logistics — is the industrialization dial. Emigration
  pressure responds to cross-bloc SoL gaps; border regimes (walls, exit
  visas) are buildable answers with world-opinion costs.
- **Resources.** Short list (~8: grain, coal, oil, steel, machine tools,
  consumer goods, uranium, cement) as regional flow balances (tons/month),
  with hard map locality from real geography (Donbas, Baku, Katanga,
  Persian Gulf). National stockpiles are first-class objects and
  intelligence targets. No entity-level logistics: capacitated,
  severable trade/transport links only.
- **Electricity.** Named regional grids: generation capacity (coal /
  hydro / oil, later nuclear) vs demand from industry + cities. Deficit
  softly throttles all consumers in the grid (~50% floor, never a hard
  stop). Interconnection is buildable. Enrichment plants are grid-scale
  consumers, welding the weapons program to the power system.
- **Industry.** A 3–4 link chain (coal/ore → steel → machine tools /
  consumer goods → capital formation) at regional resolution, with named
  industrial complexes (Ruhr, Urals, Manchuria) as the legible units.
  Every asset: capital cost to build, flow costs to operate, and
  depreciation — both blocs must run to stand still (Soviet stagnation
  emerges, unscripted). Progression by step-change production methods,
  never stacking percentages. Retooling friction à la HoI4 production
  efficiency for military lines.
- **Construction.** A national/regional capacity pool consumed as a flow
  (cement + steel + labor), gated by inputs it cannot buy itself.
  ~One construction decision per game-month; named **Great Projects**
  (Volga-Don, Aswan bids, Interstate Highways) with timelines, overrun
  risk, sabotage vulnerability, and influence payouts on completion.
- **Agriculture.** Subsistence farming is the default occupant of arable
  land (most of humanity in 1950). Province output = land × rural labor ×
  mechanization × fertilizer × weather variance. Food flows through
  procurement/transport to cities; localized famine is possible and is a
  political catastrophe, not a number.

## The planned interface (sets quantities)

- Writes **Five-Year Plans** at ceremonial paused moments: output quotas
  and material/labor/construction allocations by sector and region.
  Chunky discrete allocations, never sliders.
- Material balances instead of budgets; domestic money is soft. **Hard
  currency** (gold, oil, arms, grain exports) is the scarce external
  resource that buys Western tech and grain.
- Failure modes: shortage cascades, queues, black markets, quality malus,
  hoarding — and **falsified fulfillment reports**: the dashboard shows
  plan data, not truth; audits/purges are the internal espionage game.
- Signature moves: collectivization (fast extraction, famine risk,
  peasant resistance), Stakhanovite/forced-labor surges (welfare → output
  now, unrest debt later), crash megaprojects, border closure.
- Mobilization is fast and total; consumer goods are chronically
  squeezed, paid for in legitimacy.

## The market interface (sets parameters)

- Steers autonomous firms via taxes, interest rates, tariffs, subsidies,
  and procurement contracts. Firms' investment logic is **simple,
  published, and correct** so steering is legible.
- Hard budget constraint, bond markets, credit ratings. Business cycles
  are real: recessions, unemployment, inflation — each feeding the rival's
  propaganda.
- Trade at floating world prices (scarcity moves price; oil shocks
  emerge). Export controls (COCOM) and credit lines are economic weapons.
- Mobilization is slow and politically expensive (Korea 1950 rearmament);
  consumer abundance is the default and the shop window.

## Cross-bloc scoreboard

Standard of living is **relative**: each population scores its
consumption against the other bloc's visible standard, modulated by
information penetration (radio/TV, border porosity — jamming is economic
policy). Provinces with collapsing relative SoL drift toward the other
side's influence. The kitchen debate, as a mechanic.

## Nuclear program (bridge to pillars 1 & 3)

Uranium geography → enrichment (gaseous diffusion; grid-scale power
draw) or reactor-plutonium path → fissile stockpile (kg) → device →
**detectable test** → weaponization → delivery race (bombers → IRBM →
ICBM → SLBM). Consumes shared pools: construction capacity, scarce
scientist cohort, electricity. Every stage has an espionage-visible
footprint; stolen program progress is the biggest espionage payoff in
the game. Arsenal meaning flows through brinkmanship, never a damage
button. Civil reactors share the fissile base — Atoms for Peace exports
are influence tools that seed proliferation.

## Cut / deferred

- No per-province build queues, vehicles, fields, or substations, ever.
- Currency/inflation detail for market economies: start with one
  inflation pressure number, deepen later.
- Non-player economies simulate on the same substrate at a cheaper tier
  (regional aggregation, no interface).

## Open questions

- Region granularity for grids/industry: reuse strategic-region layer
  (TBD in map doc) or dedicated economic regions? Leaning dedicated,
  ~150–250 world-wide.
- Plan cadence: annual reviews within five-year frames, or strict 5-year
  cycles? Leaning annual adjustments with legitimacy cost for revisions.
- How much of the market bloc's firm behavior is per-firm vs sector
  aggregate? Leaning sector aggregates per region.
