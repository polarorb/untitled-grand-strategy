# Escalation & Nuclear Brinkmanship

Status: sketch
Pillar: 1 (primary), touches all others

## Core idea

A global **Tension** meter (0–100) plus per-conflict **escalation ceilings**.
Tension rises from provocations (blockades, shootdowns, tests, proxy
interventions) and decays slowly through detente actions. High tension
unlocks harsher options for everyone and raises the risk that any incident
cascades.

Each active conflict has an escalation ladder roughly:

1. Diplomatic incident
2. Arms/advisors to proxy
3. "Volunteer" formations (deniable troops)
4. Overt limited intervention (conventional, in-theater)
5. Theater war between great powers
6. Strategic conventional war
7. Tactical nuclear use
8. Strategic exchange (the failure state)

Rungs are gated by Tension, domestic politics, and what the *other* side has
done — matching your rival's rung is cheap; exceeding it is expensive and
world-opinion-costly. This models Korea correctly: the US fights openly under
a UN flag (rung 4) while China sends "volunteers" (rung 3) and the USSR
supplies pilots it officially denies (rung 2–3).

## Nuclear posture

- Arsenals have size, delivery methods (bombers → missiles → subs over the
  timeline), and readiness states (peacetime / alert / launch-ready).
- Raising readiness deters but spikes Tension; both sides at launch-ready
  triggers crisis mechanics (time-compressed decision events, misread
  warnings, accident rolls fed by `SimRng`).
- First use is always a *choice presented as a terrible option*, never an
  automatic consequence.

## Open questions

- Is Tension one global value, or per-dyad (USA↔SOV, USA↔PRC)? Leaning:
  one global + per-conflict ceilings, revisit after Korea plays.
- How do third parties read/exploit high tension?
- Crisis minigame pacing: real-time compressed, or auto-pause decision chain?
