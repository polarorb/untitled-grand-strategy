# Era scoring & the verdict — research readup

Researched 2026-09-03 by a 6-analyst swarm, two adversarial skeptics,
and a synthesis pass: genre victory and score systems (Paradox's
no-victory stance, Vic2's decomposed rank, Twilight Struggle's VP
track, Civ VI/VII era scores, Balance of Power, Suzerain), a Cold War
historian on how contemporaries actually kept score 1947-70, the
board-game scoring-cadence lens, scoreboard legibility, the
what-does-winning-mean design lens, and a codebase-grounded
integration analyst. Raw: [scoring-raw.json](scoring-raw.json).

Design brief: the vision promises era-scored victory across
checkpoints measuring alignment, output, prestige and avoided
catastrophes, with Armageddon a shared loss. The influence pillar now
freezes regional standings at 1955/1960/1965; nothing reads them. Give
the player "how am I doing" during the campaign, a reckoning at each
checkpoint, and a verdict at 1970 — one formula for 86 nations, and
never a winnable big war.

## Convergent principles

1. **Score the map as a signed delta from the 1950 par**, through the
   regional Presence/Domination/Control verdicts that already exist,
   never absolutely and never against a hands-off "historical par".
   The battleground set is structurally Western: absolute Twilight
   Struggle values give the West +41 in 1950 and +28 in 1965, a
   blowout; as deltas the same hands-off run reads East +13 by 1965,
   which is the CIA's 1958 aid-table reading of the Third World.
2. **Four fixed public freezes** (1955, 1960, 1965, and 1970 as the
   close); the campaign score is the sum of era deltas, so "the eras
   sum to the board since 1950" is a printable identity. No per-era
   reset (a 1955 collapse is permanent, as it was), no end-weighting
   (invites a fifteen-year turtle and a 1966-69 sprint), no auto-win.
3. **Catastrophe is a state above the score**: UNSCARRED, SCARRED,
   EXCHANGE. The funeral screen prints the dead, the initiator and how
   long the peace held, and nothing that orders the poles in any tense.
   Both skeptics flagged three analysts who printed a winner under the
   mushroom cloud ("by the ledger, Washington won"): that is DEFCON's
   Survivor mode with better prose, and it is refused.
4. **Words, not numbers.** One monthly verdict sentence with a named
   cause and a panel key, changing only at rollover, with a dead band;
   no live figure on the HUD. Every score display that works in the
   genre is a ranked word (Vic2's rank tab, Civ VI's World Rankings);
   every one that fails is a sum shown live (EU4 score, Humankind Fame).
5. **The epistemic contract holds for the score too**: own figures
   through the dashboard (REPORTED for planners), the rival's through
   economic penetration as a bracket with a Kent qualifier; a word the
   bracket cannot decide reads EVEN; the frozen card stays an estimate;
   belief sits beside truth only in the 1970 final edition. Both sides
   sincerely believed they were winning in 1957-62 because they read
   different metrics through their own estimates.
6. **One formula for every nation**; all asymmetry lives in three data
   fields — the column (from the live band; DENIED for the non-aligned
   field), the reach regions, and a scale (superpower 1, middle 2,
   minor 3). Contemporaries scored Nasser, Nehru and de Gaulle on the
   same six dimensions read from their own 1950 baseline.
7. **The score is a pure monthly fold** over existing digest-stable
   resources, the first occupant of the empty `TickSet::Resolve` that
   the architecture reserved for victory checks. The only new sim facts
   are attribution counters beside the mechanics that produce them:
   who used a weapon first, who prevailed and who stood down in a
   crisis, who proposed a treaty.
8. **The AI is score-blind** and the checkpoint writes nothing back
   into the sim that compounds. A score-reading AI turns the ledger
   into a reverse thermostat (a lead makes the rival reckless, so the
   leader sandbags) and makes the influence calibration harness a
   function of score tuning. Rewarding the leader with a slot is Vic2
   prestige compounding inside the pillar that was just de-snowballed.
9. **Legitimacy is read, never re-penalised.** Every act the score
   would otherwise punish twice — coups, exposed ops, exploitation,
   crush — is priced where it happens. Treaty spending of legitimacy is
   credited on the PEACE side, so a good peace is never a score loss
   (Austria 1955, the LTBT and the NPT were booked as prestige gains
   for both parties).
10. **The verdict is prose from named voices**: one sentence per
    byline, THE THREE THINGS THAT DECIDED IT as dated lines pointing at
    a frozen row or a fired event, and the era cards as words. Never an
    86-row ranking, a score-over-time chart, or a twelve-line breakdown
    (CK3's legacy score is the counterexample nobody can explain).

## What the skeptics killed

- **A winner under the mushroom cloud** (fatal): survival multipliers,
  Kahn tables, "the ledger is all that survived". The exchange has no
  class for anyone.
- **The hands-off world as the par**: it is one seed's sample
  (elections and coups roll), goes stale with every content commit,
  and grades passivity as "AS HISTORY". The par is tick 0.
- **Per-nation expectation rows** (344 historical judgments): the
  second scoring system relocated into data, and half of them events.
- **Checkpoint write-backs** (slots for the leader, domestic chains for
  the loser): compounding, and a regime model the sim does not have.
- **A score-reading AI**, in all four proposed forms.
- **OUTPUT as a level**: a third of the score would be a constant the
  Soviet player cannot move. Score growth since the last freeze against
  the rival's growth — the term the Soviets themselves headlined.
- **SCARRED from crushing a rising**: hands-off Moscow crushes Hungary
  from the timeline and would be capped at COSTLY without a decision.
  Crush is priced in legitimacy; SCARRED keys on attributed nuclear use
  or own dead per capita.
- **Six dated dimensions and a 36-cell table**: cohesion has no state,
  credibility double-counts legitimacy, firsts credit a date-triggered
  Sputnik nobody can race for. Four terms; the rest is newspaper
  flavour.
- **A "WHO LOST X" 3× debit**: on this sim the map moves mostly by
  timeline shove, so the hands-off USA would be humiliated by script.
  The MAP delta already scores a lost battleground; the headline stays.
- **A provocation ledger**: tension writes carry no author today; a
  cross-cutting change to design on its own.

## Historical calibration (from the historian)

Contemporaries tracked six things: the strategic balance (always as an
estimate of the rival), economic momentum (rate for the Soviets, level
and consumer standard for the Americans), the Third World count and UN
roll-calls, prestige (USIA polls from 1954, then firsts), bloc
cohesion, and crisis credibility. The dated shocks, by magnitude:
Sputnik (4 Oct 1957) is the era's largest single swing — Western
Europeans rated Soviet science ahead within weeks and Eisenhower's
approval fell about twenty points; the Gaither leak (20 Dec 1957)
manufactured a missile gap that U-2 and Corona had privately closed
and Gilpatric publicly closed on 21 Oct 1961; Suez and Hungary cost
both sides a point in the same fortnight (zero-sum failed); Cuba 1962
was claimed as a win by both; covert wins (Iran 1953, Guatemala 1954)
scored privately only; Korea's armistice was a draw booked as a loss
at home and a seismic prestige win for Beijing in Asia. The
scoreboard itself has a history: nobody scored space before 1957 or
Olympic points before Helsinki 1952, and each side chose the count
that flattered it (Tokyo 1964: the US press counted golds 36-30, the
Soviet press counted medals 96-90, both printed a win).

## The recommended v1 slice

**State**: a `Ledger` resource — the 1950 par frozen at seed, an era
card per country at each freeze (MAP, OUTPUT, STANDING, PEACE, the
catastrophe state, and the exact inputs needed for the 1970 reveal),
and the campaign end (the 1970 reckoning or an exchange). Attribution
fields beside their mechanics: nuclear first use and use counts,
crisis prevailed/stood-down counts, the treaty proposer. **Cadence**:
`update_score` first in `TickSet::Resolve`, monthly, freezing when
the influence checkpoints do (extended to 1970). **Formula**: MAP is
the board delta over the nation's reach in its own column; OUTPUT is
five-year growth in industry per head against the rival pole's (or the
field median's), one point per 5% with a cap of four; STANDING is the
legitimacy band delta, one point per 25 capped at three; PEACE credits
executed treaties (+2, cap +4), debits own dead per capita and every
attributed nuclear use (−4), and, for the pole leaders, months at the
Brink. Era grade from |S × scale|: STALEMATE / NARROW / CLEAR /
DECISIVE with GAIN or LOSS; campaign class from the sum: WON / HELD /
LOST, capped at COSTLY when SCARRED, none at all after an EXCHANGE.
**Surfaces**: THE STANDING line in the paper and a one-word HUD chip;
THE RECKONING as a pausing special edition at each freeze; THE FINAL
EDITION at 1970 with three bylines, the three things that decided it,
and belief beside the record; one added line on the funeral screen.
**Harness**: a hands-off run asserting bands, never integers — neither
pole CLEAR at 1955, both HELD, East MAP ≥ +10 over the campaign, both
UNSCARRED.

## Deferred, in intended return order

An economy calibration gate for OUTPUT (does the planned system
out-grow the market in the 1950s and slow after 1963?); crush priced
in legitimacy inside the influence mechanic; minor-power alignment
verbs so the field's column stops being exogenous; tension authorship
and a provocation ledger; contestability state and a capped "WHO LOST
X" debit; a score-aware (never score-chasing) allocator argued in the
influence doc; a par bid for difficulty and multiplayer; racing Great
Projects as FIRSTS; readable lock strength as COHESION; counter-
cyclical write-backs once a regime model exists; UNGA flavour; the
1970 DIVERGENCES column and a per-nation hall of records.

Next per convention: `docs/design/systems/scoring.md` via the
design-doc skill, citing this readup.
