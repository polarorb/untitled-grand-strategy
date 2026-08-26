# War termination, occupation & settlement — research readup

Researched 2026-08-26 by a 7-analyst swarm plus synthesis: HoI4's peace
conferences and their failure modes, the Paradox war-goal/infamy family
(EU4, CK3, Vic3, Stellaris), occupation & resistance mechanics (HoI4 La
Résistance, COIN board games, Vietnam '65), two historian lenses (how
limited wars actually ended 1945–90; what occupation and disposition
actually cost), alt-history end-state generation, and a dedicated
anti-exploit designer. Raw:
[war-termination-raw.json](war-termination-raw.json).

Design brief going in: the player occupied all of North Korea, holds
the Yalu, and the game has no answer — wars end at the line of control,
occupation is a frozen map, total victory has no politics. Requirement:
rich alt-history end states, but "transfer everything to me" must never
be free.

## The convergent principles (all or near-all analysts)

1. **Wars are fought FOR declared, visible war aims** — a small ladder
   (StatusQuoAnte → Punish → NewLine → Unify/RegimeChange), revisable
   mid-war only as a priced, provocative act. Crossing the 38th IS the
   aim upgrade, and it mechanically causes the intervention risk.
   Settlement demands are capped at the current aim.
2. **Termination is a process during the war, never a post-victory
   conference screen.** Talks open at a willingness threshold; the
   line at *signature* is the settlement line, so late offensives are
   bargaining moves (Panmunjom: 45% of US casualties came after talks
   opened). HoI4's terminal minigame is the anti-pattern.
3. **Patron consent is the master gate and the master anti-exploit.**
   Stakeholders = belligerents ∪ patrons ∪ UN. Clauses touching a
   client's existence need its patron to sign, be defeated, or be
   verifiably distracted. Patrons take concessions, never territory. A
   veto held by a live AI actor cannot be timer-juggled the way a
   numeric penalty can.
4. **Recognition is a persistent per-country status, not a one-time
   price.** Signed settlements grant recognition (truce, tension
   release, legal ownership). Unilateral changes stay unrecognized and
   bleed *forever* — tension floor, alignment drift, sponsored
   insurgency, near-zero production — discharged only by acts, never
   by idle time. (The Baltic-annexation model: the era delegitimized;
   it did not price.)
5. **No fungible war score.** HoI4's spendable score produced the
   shopping sprees and border spaghetti. Costs are convex in
   distance-from-status-quo (tension ∝ W^1.5) and denominated in
   non-convertible currencies: military position buys lines, patron
   consent buys sovereignty, legitimacy buys recognition. Status quo
   ante is near-free and actively rewarding.
6. **Occupation is live, expensive state** between armistice and
   settlement: 1–3 zones per war (never per-province), two tracks
   (military *control* vs population *alignment* — the COIN lesson),
   one policy posture per zone, garrisons drawn from the REAL army
   pool so pacification competes with the front.
7. **Insurgency needs a sponsor and a sanctuary**, welded to the
   espionage pillar: the rival funds your occupied zone's resistance
   as a covert budget line — negotiable at the table, counterable by
   counterintel. Without a sponsor it decays into an ignorable tax.
8. **The frozen conflict is first-class content, not a failure
   state.** Armistice-without-treaty is the era's signature outcome
   (Korea, Kashmir, the Golan): line fixed, claims unresolved, DMZ
   upkeep, tension floor, standing reconvene action.
9. **Acceptance math is deterministic and fully surfaced** —
   "blocked by PRC red line: US forces on the Yalu" — while red-line
   *positions* and resolve are intel-fuzzed, so the MacArthur
   miscalculation is reproducible by a player with bad intel.
10. **Exhaustion is domestic political will** — soft, asymmetric,
    never a forced peace on the player (the Stellaris sin).
11. **Settlements write persistent aftermath objects** — Treaty,
    FrozenConflict, DMZ, RevanchistClaim, NeutralizationPact,
    OccupationAuthority, scheduled plebiscites — so endings play
    differently for decades instead of being map paint.

## The recommended shape (five layers)

**War aims** (ladder + priced upgrades) → **Settlement Table**
(per-war entity from talks-open; clause grammar + RON templates;
monthly deterministic acceptance behind three hard gates: military
facts, patron consent with red lines, legitimacy) → **Occupation
zones** (control/alignment/policy, garrison + upkeep + sponsored
insurgency, near-zero production until integrated) → **Outcome
objects** (signed Treaty / FrozenConflict / unilateral ImposeSettlement
with permanent non-recognition) → **Pressure & shocks** (political
will; deadlocks broken by succession crises and elections, not smooth
attrition).

Key split decision on the annexation question: **no signable clause
ever absorbs a whole state into the victor** (client state is the
table's ceiling — matching zero 1945–90 precedent), but the unilateral
hold path always exists and produces exactly the Baltic status:
possible, permanent, and never accepted. Annexation is a fait accompli
the world refuses to digest, which is both the history and the brief.

## The Korea acid test (the player's exact position)

Six priced exits from "US holds everything to the Yalu": status quo
ante (near-free, dignified, rewarding); a Pyongyang–Wonsan line + DMZ
(months of fighting-while-talking, revanchist rump DPRK); UN
trusteeship (legitimacy-priced, elections in ~24 months decided by the
influence pillar); **neutralized unified Korea — the Austria lever**
(unification × neutralization collapses the patron penalty: win the
map, forfeit the asset); unification under the ROK with US alliance
(all three gates bind; needs surviving the Chinese counterblow plus
enormous compensation or a systemic key like a Soviet succession
crisis — and your read of the red line is fuzzed); unilateral hold (no
truce, bleeding on every channel, sustainable only as a strategic
choice). Meanwhile: PRC posture scales with non-ROK divisions on the
Yalu (garrisoning the border with ROK units is playing NSC-81/1
correctly), the ROK's own Unify aim accrues client dissatisfaction
against lesser deals, and refusing reasonable offers drains political
will. Deadlock falls back to the FrozenConflict — which is what
history did.

## Tensions resolved by the synthesis

- Annexation on the menu? → split by path (above).
- Legibility vs min-maxing → ledger *structure* visible, resolve and
  shock *timing* fuzzed.
- Conference granularity → v1 bundles one proposal; POW/withdrawal
  clauses slot in later as independently-stalling items.
- Client obstruction (the Rhee/Thieu loop) → clients in the
  stakeholder set from day one with visible dissatisfaction; sabotage
  events and Everready-style removal deferred to v2.
- Forced termination → patrons impose on AI clients; the player-as-
  client may refuse at severe explicit cost. Agency never confiscated.

## Sequencing (v1 slice)

1. War-aims ladder (everything keys off it).
2. Occupation zones (holding North Korea costs something month one).
3. Settlement Table with 5–6 RON templates and the three gates,
   exact-number ledger (fuzzing comes with intel plumbing later).
4. Outcome objects: Treaty, FrozenConflict upgrade, ImposeSettlement.
5. Recognition consequences wired into tension/influence/espionage.
6. Headless multi-year tests through the Korea fixture per template.

Deferred: PoliticalWill as a first-class pool, client obstruction,
POW-style agenda clauses, the systemic shock engine, intel-fuzzed
gauges, satellitization projects, refugee flows, patron coercion
toolkit.

Next per convention: `docs/design/systems/war-termination.md` via the
design-doc skill, citing this readup.
