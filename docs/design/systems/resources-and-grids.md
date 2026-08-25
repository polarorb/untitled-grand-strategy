# Resources & Regional Grids

Status: designed
Pillar: 4 (substrate), feeds 1 & 3 later (embargoes, uranium, sabotage)
Research: [economy-mechanics](../../research/economy-mechanics.md)
principles 5 (electricity as regional non-storable flow) and 8 (locality).

## What it is

The physical layer under the economy: **economic regions** (province
clusters, ~250 worldwide), **resource deposits** at real 1950 locations,
monthly **national commodity balances** (grain, coal, oil, steel), and
**regional electricity** where shortfall softly throttles industry. v1
computes and displays; consumers (construction, military, planning
interfaces) arrive in later steps. No inter-national trade yet — that
arrives with the two planning interfaces (bloc-asymmetric trade rules).

## Regions

Generated in mapgen: per country, provinces bucket into 15°×15° spatial
cells; cells with <3 provinces merge into the nearest same-country cell.
Region name = its most populous province. Every province carries a
`RegionId`. Expected scale: ~200–300 regions (USA ~8, USSR ~15, small
countries 1). Regions never cross country borders (colonial empires get
one region per colony cluster — correct for grids and future logistics).

## Deposits

Hand-authored table of ~50 major 1950 deposits (Donbas, Ruhr, and
Appalachian coal; Baku, Texas, Kirkuk, Maracaibo oil; Krivoy Rog and
Mesabi iron; Shinkolobwe and Joachimsthal uranium...), each with lon/lat
and abstract size 1–10, assigned by mapgen to the nearest province of the
right country. Stored on `ProvinceDef.deposits`. Uranium accumulates into
a national stockpile (inert until the weapons-program system).

## Monthly balances (`TickSet::Economy`, after demography)

Static v1 inputs: regional industry = country industry × region's urban
population share. Grain capacity from rural cohorts × terrain factor.

National, per country:
- **grain**: production = Σ rural × terrain_factor; demand = population ×
  ration. Ratio stored (famine mechanics come with agriculture step).
- **coal / oil**: production = Σ deposit sizes × extraction rate; demand:
  coal ← steel + power fuel, oil ← industry + power fuel.
- **steel**: production = industry × min(1, coal_ratio); demand recorded
  for future construction.

Regional:
- **electricity**: capacity seeded from industry share + hydro-ish base;
  generation = capacity × fuel availability (national coal/oil ratio);
  demand = industry + urban population draw. `power_factor =
  clamp(generation / demand, 0.5, 1.0)` — the soft-throttle multiplier
  (never a hard stop, per Civ VI's lesson) that future industry output
  will consume.

All integer math (fixed-point permille ratios), `BTreeMap` iteration,
digest in the determinism suite.

## Player-facing (v1)

- Province card gains region name and the region's power factor plus the
  national grain/coal/oil/steel ratios.
- **Power map mode** (M cycles Political → Terrain → Power): provinces
  colored by regional power factor, green → amber → red. The 1950 truth
  is visible immediately: the electrified West vs the unelectrified
  Third World.

## Cuts (v1, deliberate)

- No inter-regional transmission building yet (arrives with construction).
- No trade between countries; no stockpiles except uranium.
- Extraction levels static; no depletion, no prospecting.
- Effects don't yet propagate (power_factor computed but industry is
  static until the production step consumes it).

## Open questions

- Terrain factors for grain: start Plains 100 / Hills 60 / others lower;
  calibrate when famine mechanics land.
- Should colonial regions feed the metropole's national balance? v1 yes
  (one national pool per tag) — matches 1950 imperial economics; trade
  routes will later make this severable (blockade the empire!).
