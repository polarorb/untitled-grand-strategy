# Economic agency & regional legibility — research readup

Researched 2026-08-26 by a 7-analyst swarm plus synthesis: Victoria 3's
economy-as-game loops and their tedium collapse, construction agency
across the genre (HoI4, EU4, Stellaris, Terra Invicta, Workers &
Resources), regional information design (Vic3 states, city-builder
overlays, inspector patterns), a Cold War economic historian
(1950–65's actual decision menu), planned-vs-market asymmetric feel,
projects as the unit of agency, and a codebase-integration analyst.
Raw: [economic-agency-raw.json](economic-agency-raw.json).

Design brief: the economy architecture is locked
([economy-mechanics](economy-mechanics.md)) and the substrate runs,
but the player watches it — country-scalar numbers, a few quota
levers, regions uninspectable. Convert watching into playing without
rebuilding the sim or betraying the asymmetry thesis.

## Convergent principles

1. **No panel without a verb.** Information and action ship in the
   same slice; every dossier row terminates in a button, a deep link,
   or an inline "why". Shipping the inspector alone *worsens* the
   watching-a-simulation complaint.
2. **The unit of regional legibility is the computed binding
   constraint.** Print the argmin as a verdict sentence — "URALS
   OUTPUT LIMITED BY POWER: GENERATION COVERS 82% OF DEMAND" — not a
   flow table. At 250 regions this converts a data lake into a triage
   list. (Vic3 makes you infer the bottleneck; its most-cursed bugs
   live where the constraint chain went illegible.)
3. **Decision count is a designed budget** (~2–4 economy decisions
   per game-month, fixed by slot caps), never scaling with region
   count. Regions are where you LOOK; a capped portfolio is where you
   ACT. Verbs address regions and named complexes, never provinces.
   (Vic3's 50-state whack-a-mole ×5 is the warning.)
4. **The core verb is placing named projects on regions**, priced as
   standing flows against a construction pool gated by materials it
   cannot buy — so delays are emergent and attributable — and
   completions are step-changes the substrate models (MW on a named
   grid, capacity in a ledger), never +5% modifiers.
5. **The asymmetry lives in the decision stream, not the lever
   list.** The planner initiates and commands quantities in named
   places. The market player never places industry: they authorize
   public works and tilt a PUBLISHED deterministic investment
   allocator (zones, rates), and the dossier attributes where capital
   went and why. Vic3's despised private queue — "the same game
   played worse by an AI" — is the tombstone to steer around; the
   re-centralizing mods are the evidence.
6. **Epistemic texture is the pillar's signature UI feature.** The
   planner's regional figures are REPORTED (lies discoverable, never
   silently wrong); the market player's figures are true but lagged
   ("SURVEY, Q4 1949"); foreign regions render through espionage
   fidelity at 2 significant figures. The sim never fabricates
   quantities the inspector can't account for.
7. **Monthly, date-stamped, still display** with a curated heartbeat:
   3–6 severity-ranked wire lines per month, each with a RegionId and
   a cause. Per-tick wiggling numbers ARE the watching-a-simulation
   feeling, rendered in UI.
8. **Build by thawing, not rebuilding**: promote the init-frozen
   regional industry distribution to live serialized state, route
   part of existing investment through the pool, keep auto-invest as
   the AI/passive default — all 226 AI countries keep current
   behavior with zero new AI code. The military command layer is the
   proven template: player decides what/where, sim executes.
9. **Placement is only a decision if sites differ**: endowments
   (idle labor, power surplus, deposits) visibly modify project
   cost/speed, and Great Project offers are condition-gated
   predicates over sim state, not an era menu (HoI4's solved-opening
   focus trees as the anti-pattern).

## The recommended v1 slice

**Thaw**: `RegionalIndustry` as live state; per-region power factors
actually throttle their own region; monthly `RegionSnapshot` with a
`ConstraintKind` verdict. **See**: BOTTLENECK map mode; a National
Economy Ledger (one sortable row per region, bloc-specific columns —
planner PLAN/REPORTED, market lagged surveys); the Region Dossier — a
fixed one-page teletype document, sibling of the battle inspector,
with a verdict footer and verb buttons. **Act**: a construction pool
(directed share of existing investment; surplus auto-invests as
today), 2 generic + 1 Great Project slots, `StartProject`/
`CancelProject` through the command queue; planner places all kinds,
market places public works only plus development zones steering the
published allocator. **Great Projects**: ~8 historian-sourced,
condition-gated catalog entries — Volga-Don active at start (an
inherited portfolio before the first move), Kuibyshev HPP, the
Turkmen Canal as the priced trap, Virgin Lands (1954+, must be able
to fail), St. Lawrence Seaway, Interstate (1956), Paducah.
**Heartbeat**: monthly wire lines + a rollover briefing card with one
templated recommendation derived only from visible data — so the
player can learn to beat it.

## Tensions resolved by the synthesis

- Projects vs priority-lists as the planner verb → projects v1,
  priority flags v1.1 (both historically true; ship the buildable
  half; reserve dossier layout for the priority column).
- Sim-proposed vs player-initiated decisions → player-initiated; the
  briefing recommendation is the thin end of the proposal wedge.
- Overruns → generic projects purely emergent (attributable
  shortfalls); Great Projects add seeded, attributed event rolls —
  Virgin Lands must be able to fail, a routine power station must not
  read as dice.
- Epistemic scope → v1 shows PLAN/REPORTED via the existing national
  padding distributed proportionally (the player learns in minute ten
  the dashboard can lie); per-region drift + AUDIT verb + diegetic
  tells (queues, black-market index) are the first fast-follow, since
  a lie without tells reads as a bug.
- Market texture → zones + attributed digest v1; the Contract Book
  (named complexes bidding on procurement) is the market side's first
  follow-up. Watch decision-budget parity between blocs.

## Deferred, in intended return order

Contested foreign bids / the Aswan auction (reuses project machinery,
fuses pillars 2–4 — best post-v1 payoff); falsification drift + audit
game; commodity priority flags; the Contract Book; hard-currency
funding (blocked on dual currency); sovnarkhoz administrative reform;
full proposal engine; SoL/food map modes; labor drafts and storming.

Next per convention: advance
`docs/design/systems/economy.md`'s agency slice (or a dedicated
`economic-agency.md` system doc) via the design-doc skill, citing
this readup.
