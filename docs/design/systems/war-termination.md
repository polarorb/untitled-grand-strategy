# War termination, occupation & settlement

Status: implemented (v1 slice)
Pillars: 1 (escalation — aims, red lines, imposed outcomes are
brinkmanship), 2 (influence — legitimacy, alignment drift, plebiscites),
3 (intel — fuzzed red lines, sponsored insurgency), 4 (economy —
occupation costs; and the domestic-annexation asymmetry IS systems
competition: different constitutions have different annexation
machinery).
Research: [war-termination](../../research/war-termination.md)
(7-analyst swarm + synthesis, 2026-08-26). Sibling docs:
[military](military.md) (armistice v1, which this supersedes and
extends), [tension](tension.md), [espionage](espionage.md),
[influence](influence.md).

Wars currently end at the line of control and occupation is a frozen
map. This system makes ending a war the strategic act: wars are fought
FOR declared aims, termination is negotiated *while fighting*, holding
conquered ground is expensive live state, and every outcome — signed,
frozen, or imposed — is a persistent object the world keeps reacting
to. Restraint is cheap and rewarding; ambition is priced continuously;
nothing, including doing nothing, is free.

## Player-facing description

- Declaring war (or being attacked) sets a **war aim** per belligerent
  from a ladder: *Status Quo Ante* (defender's free default) →
  *Punish* → *New Line* → *Unify/Regime Change*. Upgrading mid-war is
  one click with the price printed on it: tension, legitimacy, and a
  visible shift in every patron's disposition. Crossing the 38th IS
  the aim upgrade.
- When either side becomes willing to talk (the war room announces
  it), the **Settlement Table** opens as a war-room tab: propose a
  package built from clause templates (status quo ante, new line +
  DMZ, client state, trusteeship, neutralized unification,
  unification, incorporation), see every stakeholder's acceptance
  ledger — each row a named reason ("BLOCKED — PRC RED LINE: US
  FORCES ON THE YALU BORDER"), v1 with exact numbers. Fighting
  continues while talks run; the line at signature is the line you
  get, so a last offensive is a bargaining move.
- Territory you hold in an enemy country is an **occupation zone**:
  one card per occupied nation showing military *control* vs popular
  *alignment*, the garrison requirement it pins (real divisions,
  visibly absent from the front), the monthly bill, and a policy
  posture. Insurgents — funded by your rival, over a porous sanctuary
  border — flare into events, not rebel stacks.
- Three ways out: **sign** (treaty executes, truce holds, tension
  releases, recognition granted); **freeze** (armistice without
  treaty: DMZ, tension floor, revanchist claims, a standing
  "reconvene" action — Korea 1953, first-class outcome); or
  **impose** (keep what you hold: no truce, permanently unrecognized,
  bleeding on every channel until discharged by later acts).

## Layer 1 — War aims

State, on `Military`:
`war_aims: BTreeMap<(CountryTag, CountryTag), WarAim>` keyed
(belligerent, enemy);
`enum WarAim { StatusQuoAnte, Punish, NewLine, Unify }` (ladder index
0–3). Defenders get `StatusQuoAnte` free at war start; attackers
declare via the war declaration's source (scripted events set aims;
`korea-invasion` sets PRK → Unify).

Command `SetWarAim { country, enemy, aim }` (upgrades only; lowering
is free and instant):

- Tension cost on upgrade: `AIM_TENSION[rung] = [0, 10, 25, 60]`
  internal tenths, paying the *difference* of rungs.
- Legitimacy delta: `AIM_LEGITIMACY[rung] = [0, 0, -10, -25]`, waived
  if a scripted UN-mandate event pre-authorized that rung (the Oct 7
  1950 UNGA resolution analog).
- Patron recompute: every stakeholder's red-line check re-evaluates
  the tick after (see Layer 2).

Settlement demands are **capped at the current aim**: a package's
total sovereignty weight `W` (below) may not exceed
`AIM_MAX_W[rung] = [2, 4, 10, 26]`. You may always offer less.

## Layer 2 — The Settlement Table

One table per **conflict** — the connected component of the war graph
(Korea's three war-pairs are one conflict). State, new resource
`Settlements`:

```rust
struct Conflict {
    members: BTreeSet<CountryTag>,          // belligerents
    stakeholders: BTreeSet<CountryTag>,     // ∪ patrons ∪ UN flag
    talks_open: bool,
    proposals: Vec<Proposal>,               // standing, ≤1 per proposer
}
struct Proposal { proposer: CountryTag, clauses: Vec<Clause>, since_tick: u64 }
enum Clause {
    BorderChange { from: CountryTag, to: CountryTag, provinces: Vec<ProvinceId> }, // W=2
    Dmz { provinces: Vec<ProvinceId> },                                            // W=1
    ClientState { state: CountryTag, patron: CountryTag },                         // W=6
    Trusteeship { state: CountryTag, admin: CountryTag, review_months: u16 },      // W=4
    Neutralization { state: CountryTag },                                          // W=-6 (a concession)
    Unification { absorbed: CountryTag, under: CountryTag },                       // W=12
    Incorporation { territory: CountryTag, annexer: CountryTag },                  // W=18, domestic gate
    Reparations { from: CountryTag, to: CountryTag, stock: u64 },                  // W=1
    RecognitionGrant { of: CountryTag },                                           // W=-3 (a concession)
}
```

Talks open when the existing armistice-willingness heuristic fires for
*either* side of any war-pair in the conflict (repurposed: it no
longer ends the war). Templates live in
`assets/data/scenario/1950/settlements.ron` as named clause bundles;
the UI offers templates, the grammar permits hybrids
(Unification × Neutralization = the Austria lever).

**Acceptance** — monthly (`SimClock::new_month`), `TickSet::Politics`
after `update_events`, deterministic integer math, every term shown in
v1. A proposal signs when every *required* stakeholder accepts.
Required = both war-pair principals touched by any clause, plus every
gate-holder below. Per stakeholder, three hard gates then a utility
comparison:

1. **Military facts**: territorial clauses (BorderChange, Unification,
   Incorporation) require the proposer's side to hold ≥
   `OCCUPY_GATE_PERMILLE = 800` of the named provinces.
2. **Patron consent**: clauses ending or transferring a client's
   sovereignty (ClientState, Unification, Incorporation, Trusteeship)
   require the client's patron to accept — or be defeated (its own
   army broken) or *distracted* (at war elsewhere / in crisis, v2
   systemic shocks). Patron = its bloc's superpower (USA / SOV) plus
   any great-power co-belligerent (PRC for PRK). **Red line**: a
   stakeholder whose home border is adjacent to provinces holding
   another superpower-bloc's non-local divisions applies
   `RED_LINE_PENALTY = 60` to its utility — compensation cannot fully
   offset it while that stakeholder's army stands. (Garrison the Yalu
   with ROK divisions and the penalty lifts: the NSC-81/1 play.)
3. **Legitimacy**: proposer pays `W` from a per-country
   `legitimacy: i32` stat (UN-mandate and coalition events grant it;
   range −100..100, start 0). Insufficient legitimacy blocks
   high-W clauses with the shortfall printed.
4. **Domestic consent (Incorporation only)** — see below.

Utility (accept iff `≥ 0`):
`U = terms_value + compensation + exhaustion − line_prospects − red_line_penalty`
where `terms_value` = signed sum of clause weights from that
stakeholder's perspective; `exhaustion` = `war_months +
casualties_permille_of_population * 2 + occupation_drain_months`;
`line_prospects` = `FRONT_MOMENTUM_SCALE = 10` × sign of the last 60
days' net province flips for its side. Acceptance difficulty scales
convexly: the proposer's package must also clear
`W_PRICE = W * isqrt(W * 100) / 10` (≈ `W^1.5`) paid in tension added
at signature *if imposed on a reluctant minor* — signed packages
between willing parties *release* `SETTLE_TENSION_RELIEF = −50 − W`
(bigger settlements calm more, because more is resolved).

### The domestic-consent gate (player feedback, incorporated)

Annexation (`Incorporation`) is on the table for every government —
priced by its own constitutional machinery. This is systems
competition inside war termination:

- **Command systems** (`EconomicSystem::Planned`): no domestic block —
  the Supreme Soviet admits new republics by decree (Baltics, 1940).
  Price is external and permanent: `Incorporation` by a Planned
  annexer costs double legitimacy (`2W`) and adds
  `ANNEX_TENSION_FLOOR = +30` tenths to the tension floor while any
  non-bloc power withholds recognition.
- **Market democracies** (`EconomicSystem::Market`): the clause passes
  only if BOTH hold: zone `alignment ≥ INCORP_ALIGN_GATE = 400` (the
  territory consents) AND territory population ≤
  `INCORP_POP_CAP_PERMILLE = 30` of the annexer's (Congress will
  ratify a Guam, not a Korea — the Treaty of Paris cleared the Senate
  by one vote *for islands*, and the Insular Cases immediately
  invented "unincorporated territory" to keep the inhabitants at
  arm's length; the Anti-Imperialist League's opposition was real and
  partly xenophobic, and the Philippines were promised independence
  by 1934). Over the cap, the ledger shows the block by name:
  "CONGRESS WILL NOT INCORPORATE 9.6M KOREANS — REQUIRES DOMESTIC
  POLITICAL TRANSFORMATION." That transformation — incorporation
  referenda, government-type change, a palace coup that swaps the US
  constitutional order — is a real designed pathway, **post-slice**
  (below), not a dead end.

## Layer 3 — Occupation zones

Replaces frozen-map occupation as the *political* layer (the
`occupation` province map remains the military layer). State, on
`Military`: `zones: BTreeMap<(CountryTag /*holder*/, CountryTag
/*original*/), OccupationZone>`, created when a holder holds ≥
`ZONE_MIN_PROVINCES = 3` of another country, dissolved by treaty or
liberation.

```rust
struct OccupationZone {
    control: u16,      // 0..1000, military grip
    alignment: i16,    // -1000..1000, popular disposition toward holder
    policy: ZonePolicy, // MilitaryGovernment | ClientAdministration | Exploitation
    insurgency: u16,   // 0..1000, derived pressure
}
```

Daily (`TickSet::Military`, inside `update_command`):

- Garrison requirement `= zone_population / GARRISON_MEN_PER = 250_000`
  men (≈ 1 division per 2.5M), counted from divisions located in zone
  provinces. Shortfall → `control` −`CONTROL_DECAY = 3`/day; met →
  +2/day toward `600 + policy bonus`.
- `insurgency = (1000 − control) / 4 + max(0, −alignment) / 4 +
  SANCTUARY_BONUS (200 if adjacent to a hostile-or-rival-bloc border)
  + sponsor_tap` where `sponsor_tap` is a rival espionage budget line
  (new `OpKind::SponsorResistance`, 0–300 by funding level,
  suppressed by the holder's counterintel).
- Insurgency ≥ `FLARE_GATE = 500` rolls weekly events from
  `rng.fork(b"occupation")`: garrison casualties (strength debits),
  supply raids (stock), atrocity decisions (control vs alignment
  trades). Never spawned rebel formations.
- Monthly: upkeep `= provinces * ZONE_UPKEEP_CENTI = 15` centi-stock
  (on top of overseas division upkeep); alignment drifts by policy —
  MilitaryGovernment +control −alignment, ClientAdministration
  −control +alignment (needs a same-bloc local client tag),
  Exploitation +stock −alignment −legitimacy.
- Occupied provinces produce **zero** industry/agriculture for anyone
  until a treaty assigns them (anti-exploit: conquest must not fund
  its own penalty).

## Layer 4 — Outcomes

- **Treaty** (`Settlements.treaties: Vec<Treaty>`): executes clauses —
  province ownership transfers become *legal* (a new
  `legal_owner: BTreeMap<ProvinceId, CountryTag>` overlay the economy
  and demography read), client states re-tag alignment, DMZ entities
  demilitarize provinces (divisions may not enter), trusteeships
  schedule review events, plebiscites schedule influence-pillar
  contests. Grants truce `TRUCE_MONTHS = 60` between signatories and
  per-signatory `recognition`. Tension release as above.
- **FrozenConflict** (the upgraded existing armistice): military
  demarcation line + `DMZ_DEPTH = 1` province strip each side,
  tension floor +`FROZEN_TENSION_FLOOR = 10`, `RevanchistClaim` on
  the cut-off side (feeds future events), a standing reconvene action
  whose gates re-evaluate monthly. This remains the automatic outcome
  when both sides are willing but no package clears — old behavior
  preserved, now with furniture.
- **ImposeSettlement** (command): shooting stops, NO truce (the enemy
  may resume at will, cheaply), holdings enter
  `unrecognized: BTreeMap<CountryTag /*observer*/, BTreeSet<ProvinceId>>`
  for every non-bloc observer. While any holding is unrecognized:
  tension floor +`ANNEX_TENSION_FLOOR`, monthly world alignment drift
  −`UNRECOGNIZED_DRIFT = 2` permille toward the rival bloc (influence
  pillar), rival `SponsorResistance` discounted 50%, production stays
  zero. Discharge only through later treaty, compensation packages,
  or generational events (v2) — never idle time.

## Cadence & placement

Aims/commands: `TickSet::Commands` flush. Zones: daily in
`update_command`. Table evaluation + outcome execution: monthly,
`TickSet::Politics`, a new `update_settlements` system chained after
`update_events` (wars/aims set by events that tick must be visible).
`settle_wars`'s willingness heuristic moves into talks-open + the
FrozenConflict fallback; total-collapse handling becomes zone creation
plus an auto-opened table.

## Interactions

- **Tension**: aim upgrades, imposed outcomes, floors; settlements
  release. **Influence**: legitimacy, alignment drift, plebiscites,
  trusteeship reviews. **Espionage**: SponsorResistance op,
  counterintel suppression; post-slice, red-line positions become
  intel-fuzzed estimates. **Economy**: zone upkeep, zero occupied
  production, reparations move `military_stock`; Planned-vs-Market
  domestic gate. **Military**: garrison requirements compete with
  theaters for real divisions; DMZ provinces bar entry; truce blocks
  re-declaration. **Events**: scripted aims, UN mandates, patron-aid
  `GrantStock` continue to work; `TransferProvinces` becomes a treaty
  clause under the hood.

## AI note

AI proposers evaluate the template list monthly with their own utility
and propose the best package clearing their gates; AI stakeholders
answer with the same visible math. Patrons prefer concessions to
territory always. AI clients hold their scripted aim and accrue
visible dissatisfaction against lesser deals (obstruction events
post-slice). The AI never imposes unilaterally unless its opponent has
collapsed AND no patron objects.

## Edge cases

- Total collapse (current "resistance ends"): zone over the whole
  country, table opens with every military gate satisfied — the
  disposition still needs patron/legitimacy/domestic gates. Occupation
  costs run meanwhile.
- Multi-war belligerents: conflicts merge when a new war connects
  them; treaties only bind signatories — separate peace stays legal
  (and the bloc-basing rule keeps it playable).
- Zone with zero garrison: control → 0, insurgency near max,
  liberation events can flip provinces back without combat.
- Proposer at aim cap: UI blocks assembling over-cap packages with the
  aim named.
- Legitimacy at −100: only W ≤ 0 packages (concessions) proposable.
- Determinism: all BTree state, monthly cadence, forked RNG streams,
  everything in the digest.

## Deliberately not modeled (v1 cuts)

- Political-will as a first-class pool (v1 reuses exhaustion inputs).
- Client obstruction (Rhee/Thieu loop), covert leader removal,
  guarantee packages — stakeholder data model reserves the slot.
- POW repatriation / foreign-force-withdrawal clauses that stall
  packages independently (Panmunjom pacing) — clause enum reserves.
- Systemic shock engine (succession crises, elections) and
  superpower ForcedTermination compression — **decided**: patrons
  impose on AI clients; the player-as-client may refuse at severe
  explicit cost (patronage severed). Post-slice.
- Intel-fuzzed acceptance gauges (v1 shows exact numbers — decided).
- Refugee flows, satellitization projects, trusteeship elections as
  full influence contests (v1 schedules stub events).

## Post-slice: domestic transformation (player feedback, reserved)

The Incorporation gate assumes each government's *current*
constitutional machinery. The designed extension: a `GovernmentType`
axis independent of economic system, changeable by coup/revolution
events (including in the player's own country — a palace coup that
swaps the US to a government type with different annexation
machinery), incorporation referenda as influence contests, and
domestic factions (anti-imperialist blocs with period-honest motives)
as event actors. The v1 data model must not hard-code
`EconomicSystem` as the gate's key — route it through a
`domestic_annexation_gate(country) -> GateKind` function so the axis
can swap in without rework.

## Implementation deviations (v1, deliberate)

- **Templates are code builders, not RON data**: clauses bind to the
  live line, zones, and belligerents at proposal time, which RON
  cannot express. The clause grammar itself is the data surface.
- **The sponsor tap reuses espionage collection networks** (funding
  level vs the holder) instead of a dedicated `SponsorResistance` op —
  an honest reuse; the dedicated op arrives with the espionage
  expansion.
- **Insurgency flares roll daily at one-seventh weekly odds** (the
  determinism rules ban tick-modulo weekly gates).
- Neutralization and ClientState clauses are recorded with wire
  notices; their mechanical teeth (basing exclusion, orbit effects)
  land with the influence pillar.
- Trusteeship reviews and plebiscites fire as scheduled notices, not
  yet influence contests.
- Treaty-ceded territory does not yet transfer economic output —
  industry is a country scalar until per-province industry exists;
  occupation zeroing applies to the loser either way.
- Unrecognized-holding alignment drift toward the rival bloc awaits
  the influence system; v1 prices non-recognition through the tension
  floor, zero production, and the discounted sponsor tap.
- One package signs per month (ordering simplicity); the UI ledger
  shows exact numbers per the decided v1-legibility cut.

## Open questions (leanings)

- Should DMZ provinces demilitarize for *both* signatories' blocs or
  all countries? Leaning: signatories + their blocs.
- Reparations denominated only in `military_stock`, or also industry
  transfer? Leaning: stock-only v1.
