# Agriculture & Procurement

Status: designed (v1 slice)
Pillar: 4, feeding 2 (famine as propaganda ammunition, later)
Research: [economy-mechanics](../../research/economy-mechanics.md) —
the W&R "chain-shaped famine beats RNG famine" lesson, Victoria 3's
subsistence default, the collectivization gamble every analyst demanded.

## v1 slice

Grain production (already: rural cohorts × terrain) gains **weather**,
**agricultural policy**, and **consequences**. The Great Leap famine must
be a reachable outcome of player choices, not an event.

### Weather

Each January, every country rolls a **harvest factor** (850–1150‰,
uniform) from a forked `SimRng` stream — the sim's first real use of
seeded randomness. Applied to all grain production that year. Planned
economies fear the weather; that's historical and intended.

### Policy (planned economies only — command `SetAgriPolicy`)

- **Procurement quota** (Low / Normal / High): multiplies effective food
  extraction (900 / 1000 / 1100‰). High quotas squeeze more grain out of
  the countryside — and when the harvest is bad, the extra famine deaths
  concentrate on the **rural** cohort (70/30 split vs even). Khrushchev's
  dilemma as a slider.
- **Collectivization** (toggle, one-way ratchet in v1): +150‰ extraction
  efficiency permanently, −100‰ base yield permanently, and a **12-month
  transition shock** of an additional −250‰ yield. Fast state grain at
  famine risk — the historical gamble, priced honestly.
- Market economies: no controls in v1 (price supports and the farm lobby
  arrive with trade); they take weather like everyone else but their
  famine deaths never concentrate (no forced procurement).

### Consequences (monthly, after production)

`food_ratio = national grain ratio × quota extraction`:

- ≥ 1000: +2 SoL (well-fed bonus)
- 900–1000: neutral
- 750–900: SoL penalty scaling to −12 (shortage, rationing)
- < 750: **famine** — SoL −15 and excess deaths at
  `(750 − ratio) × 40` ppm/year applied to cohorts (rural-weighted under
  High quota), tallied in a cumulative famine-deaths counter per country.

## State & placement

`Agriculture` resource: per-country policy, current harvest factor,
food ratio, famine flag, cumulative famine deaths. Runs LAST in the
`TickSet::Economy` chain (demography → balances → production →
agriculture) so it adjusts the SoL that planning just wrote and applies
deaths directly to `Demographics`. Weather rolls iterate countries in
`BTreeMap` order from a stream forked per-January — fully deterministic.

## UI (v1)

Economy panel gains a food section: FOOD ratio, HARVEST factor, and for
planned players the QUOTA cycle + COLLECTIVIZE button; a red famine
warning line when active.

## Cuts

- No grain trade/imports yet (a famine you could have imported out of
  arrives with trade — and becomes the humiliation mechanic).
- No Virgin-Lands-style campaigns, fertilizer/mechanization tech.
- De-collectivization, peasant resistance events.

## Open questions

- Should market economies get an emergency "food aid request" action
  before trade lands? Leaning no — let the gap be visible.
