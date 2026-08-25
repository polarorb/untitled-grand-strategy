# Military

Status: sketch
Pillar: serves 1 (escalation) primarily — war is the pillar's expression

## Core idea

HoI4-inspired but posture-first: most of the game, armies deter rather than
fight. Division-scale units, province movement, front lines when war comes —
but less micro than HoI4 (fewer, larger provinces per theater; orders at
corps/army level with unit autonomy underneath).

- **Readiness over mass**: peacetime armies are cadres; mobilization is a
  visible, provocative, expensive act (feeds Tension).
- **Limited war rules**: rules of engagement follow the conflict's
  escalation rung — sanctuary borders (no bombing across the Yalu unless you
  escalate), deniable "volunteer" formations fight under the proxy's flag.
- **Combat resolution**: deterministic given seed; soft attrition model with
  supply, terrain, and doctrine modifiers. Design detail deferred until the
  Korea vertical slice forces the issues.
- **Navies & air**: presence and interdiction abstractions first (carrier
  task forces as influence/escalation tools); detailed naval combat later.

## The Korean War as design driver

Korea is the vertical slice: a limited war with great-power participation
under escalation constraints, amphibious flanking (Incheon), intervention
tripwires (approaching the Yalu), and a negotiated stalemate as a *valid
outcome*. If the systems can produce a satisfying Korea, they generalize.

## Open questions

- Division designer? Leaning no — national doctrine + equipment era instead.
- Front-line automation level vs. HoI4 battle-planner?
- How do armistice/stalemate mechanics work — war weariness, line stability?
