# Espionage: knowing things, doing things, and the price of being caught

*Research swarm, 2026-08-25 — five analysts on genre spy mechanics, the
intelligence-quality economy, sourced 1945-1962 tradecraft, operation
design & risk legibility, and pillar integration. Raw findings in
[espionage-raw.json](espionage-raw.json).*

The vision line this pillar serves: *what your rival believes matters as
much as what is true; deniability is a currency.* The nuclear pillar
already computes deterrence from biased estimates — this layer is the
other half: the machinery that moves those beliefs, on both sides.

## The convergent principles

**1. Every shipped grand-strategy espionage system broke the same two
ways — and the fixes are known.** HoI4 La Résistance is the canonical
failure: mandatory maintenance loops (network decay, capture-rescue
side-quests) plus payoffs too weak to change decisions — "spy system too
tedious" is a literal forum thread title. EU4's passive networks were
good but its active ops weren't worth the spend; Stellaris was
simultaneously a click-fest and fire-and-forget (proof that click count
and engagement are orthogonal). Rules: **networks never decay from
inaction, upkeep is never a player job, and every espionage output must
change a decision the player was already making.** Decisions should be
pull (opportunities arrive), never push (system demands attention).

**2. Twilight Struggle got the feel right by having no spy system at
all.** Covert action there is the opportunity-cost structure of the
whole game: every covert act competes for the same scarce action, has a
visible price, and spends escalation headroom (coups degrade DEFCON).
Espionage must live *inside* the brinkmanship pillar, not beside it.

**3. Intent-then-event cadence.** Orders take seconds (a standing
collection posture, a launched op); product returns as irregular,
story-shaped teletype events — a decrypt, a walk-in defector, a blown
asset — each demanding a real choice. The existing wire engine is the
delivery channel. Never a silent modifier change.

**4. Risk is a legible clock, not a die roll.** Invisible Inc's visible
security level and Phantom Doctrine's monotonic danger meter beat
percentage-roll espionage everywhere. Our per-program exposure
accumulator becomes the load-bearing mechanic: every use of a network
adds known exposure, thresholds are inspectable, and the player can
always answer "how burned am I?" before acting.

**5. Show outcome ladders, never success percentages.** Before
commitment: 3-5 concrete named world-states (clean / success-with-
evidence / quiet failure / loud failure / catastrophic exposure) with
period estimate-language likelihoods (PROBABLE / POSSIBLE / UNLIKELY)
derived from inspectable factors. The player gambles on *which branch*,
never on whether the UI lied (the XCOM 95%-miss betrayal).

**6. Failure is content.** Every loud failure spawns a U-2-shaped
multi-week arc: wire flash with facts unclear → a cover-story choice
made *before* you know what evidence the enemy holds (the NASA weather-
plane statement ran while Powers sat alive in Moscow) → enemy-timed
reveal → show trial / summit collapse. Being caught lying costs
strictly more than early admission.

**7. Attribution is a three-state machine driven by named evidence
objects.** UNATTRIBUTED → SUSPECTED → PROVEN, moved by discrete things
("pilot in custody", "shipment with factory markings", "defected
operator"), not a meter. Only PROVEN works as a crisis pretext — and
per the scholarship (Cormac & Aldrich), even proven-but-unacknowledged
ops escalate less than acknowledged ones: the *fiction* of deniability
is itself an escalation damper, serving total-war-is-failure directly.

**8. Four intel domains, because four consumers already exist.** One
penetration score per (viewer, subject, domain) — **nuclear** (feeds
the deterrence opacity bias), **military** (replaces the war UI's
hardcoded fuzz widths), **economic** (sees through planned economies'
reported-vs-actual books), **political** (crisis resolve bands). WIDTH
shrinks with any intel; BIAS shrinks only with verification-class
sources (overflights 1956+, satellites 1960+) — calibrated to the
bomber gap: opacity ~1.8× plus parade deception ~+0.8× = 2.6×,
collapsed by a single overflight the way one Saratov-Engels photo did.

**9. No agent roster — names are narrative, not units.** Named agents
as managed units fail everywhere (HoI4 operatives, Terra Invicta's
"councilor micro is not fun"). Players track ~5 espionage threads max.
Cap: 3-4 standing country networks + 1-2 op slots. Named individuals
appear only as event-borne singular instruments — a codenamed mole
with a product stream and an exposure clock, burned once, mourned on
the wire.

**10. The best payoffs are held instruments and belief changes, not
instant effects.** CK3's hooks (leverage) over CK3's murders; a
penetrated ministry that keeps feeding truth until burned; kompromat
spendable in a crisis. And uniquely ours: narrowing a fuzz range,
piercing the nuclear opacity bias, exposing a planned economy's lie.

**11. The enemy plays your dead networks back.** Denied-area
infiltration failed at 80-95% historically (Albania: 37+ of 83 agents
lost; Ukraine: 11 of 12; China: the team was doubled and the pickup
flight ambushed) — and rolled-up networks kept transmitting under
enemy control. A blown network should keep showing product and keep
feeding *poisoned* estimates until counterintel catches it — the intel
pillar's version of reported-vs-actual. Moles are the dominant failure
cause (Philby blew Albania; Blake blew the Berlin Tunnel before ground
broke), and suspicion accumulates for years without proof.

**12. Covert action is cheap in money, expensive in politics.** Iran
1953: ~$1M released, five months. Guatemala: victory by manufactured
belief (the Voice of Liberation made a tiny force unstoppable — the
government's will broke against a *perception*). The Berlin Tunnel:
$6.7M, blown from day one, still yielded 443,000 conversations. Price
ops in deniability, tension, and blowback; let cash be almost trivial.

## Tuning anchors (sourced)

- **Institutions**: CIA ~5,000 staff Jan 1950 → >10,000 by 1953; the
  covert arm went 302 staff/$4.7M (1949) → ~6,000/$82M (1952). Hot war
  is the step-change funding unlock (NSC-68/Korea).
- **Source shapes**: embassy/attaché baseline (free, safe, cap ~25);
  agent networks (workhorse; sole access to economic truth and
  resolve; roll-up risk vs closed societies); SIGINT/decrypts (Venona
  read <1% of traffic, years stale, unusable in court, blown by moles
  anyway — retroactive product, killed by cipher changes); overflights
  (1956+: per-mission certainty purchases with shootdown risk growing
  with SAM tech — ~24 deep penetrations before Powers); satellites
  (1960+: 12 failures then Discoverer 14 covered more than every U-2
  flight combined; risk-free, ends strategic fog, sees only hardware).
- **Event spine**: Fuchs trial (Mar 1 1950), McCarthy's Wheeling
  speech (Feb 9 1950), Rosenberg arrests (Jul-Aug 1950) and execution
  (Jun 19 1953), CIA's Korea surprise failures (Jun & Nov 1950),
  Burgess/Maclean flight (May 25 1951), Beria's fall opening the
  defection window (1953), Petrov (Apr 3 1954), Berlin Tunnel
  (May 1955-Apr 1956), U-2 shootdown killing the Paris summit
  (May 1-16 1960), Corona deflating the missile gap (Aug 1960) —
  truth arriving can *lower* tension.

## The recommended architecture (consensus)

- `IntelState`: BTreeMap<(viewer, subject), {nuclear, military,
  economic, political}> permille. Growth: monthly asymptotic approach
  to the best active source's cap (`intel += (cap-intel)*rate/1000+1`);
  decay toward the remaining floor when sources lapse; verification
  expires separately so bias regrows (reproducing the May-Aug 1960
  missile-gap window).
- One abstract network per (owner, target), funded 0-3, no upkeep
  micro; sabotage and theft *spend and risk* the network that also
  collects (the OSO/OPC fratricide as one resource).
- Counterintel: structural floor by regime openness (closed ~300) +
  funded component from the same agency budget; monthly sweep rolls;
  a catch fires a spy-trial choice event — show trial / quiet
  expulsion (banks a swap) / turn the agent (feeds deception).
- Deniability counter: each blown op spends it; at low deniability the
  next blown op's tension cost multiplies (the U-2 problem).
- Deception: per-domain subject-side posture; seen through above a
  penetration threshold ("SOURCES CONTRADICT OFFICIAL FIGURES"),
  permanently marking the subject a known deceiver.
- UI: an Intelligence panel of dossier cards — coverage grade
  (NEGLIGIBLE/LIMITED/PARTIAL/EXTENSIVE), the estimate with its band,
  an AS-OF date, a provenance line. Never raw permille.

## V1 cut (the consensus commit order)

1. IntelState + the four one-line couplings (deterrence bias, economy
   reported-vs-actual lerp, war-UI fuzz widths, crisis resolve bands)
   — every existing system visibly changes before any new content.
2. Networks with funding commands; counterintel + sweeps + spy-trial
   events; nuclear sabotage & StealDesigns ops with blown-op → tension
   + crisis pretext + deniability counter; minimal defectors (spike +
   snapshot card); the Intelligence panel.
3. **v1.5**: mole hunts (the Angleton tradeoff: six months of paralysis
   to clean house), turned agents / radio playback, prisoner swaps,
   scapegoats. **v2**: overflights (mid-50s unlock, shootdown crisis
   loop), Venona-style decrypts with source-burning dilemmas.
   Coups/election ops belong to the influence pillar and should
   consume this layer, not duplicate it.
