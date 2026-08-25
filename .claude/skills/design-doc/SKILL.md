---
name: design-doc
description: Write or advance a game design doc in docs/design/systems/ — from idea to sketch, or sketch to designed (implementable spec). Use before implementing any new mechanic, or when the user wants to explore a mechanic's design.
---

# Design doc

Design docs live in `docs/design/systems/<mechanic>.md` and move through
`Status: sketch` → `designed` → `implemented`. Nothing is implemented from a
sketch.

## Ground rules

- Read `docs/design/vision.md` first. Every mechanic must serve at least one
  of the four pillars — name which, in the doc header. If it serves none,
  say so and recommend cutting it.
- Read the sibling docs it touches (escalation ↔ espionage ↔ influence are
  deeply coupled). Cross-link interactions explicitly.
- This game is NOT HoI4: when borrowing an HoI4 mechanic, justify it against
  the "What this game is NOT" list in the vision doc.
- For historical grounding, delegate to the `historian` agent rather than
  guessing dates, numbers, or sequences.

## `sketch` → `designed` checklist

A doc is `designed` when an implementer could build it without asking
design questions:

- [ ] Player-facing description: what the player sees, decides, and feels.
- [ ] State: what data exists, on what entity (world / country / state /
      province / unit), with types and ranges.
- [ ] Cadence: hourly / daily / monthly, and which `TickSet` stage.
- [ ] Formulas: actual math with named tuning constants and starting values.
- [ ] Interactions: which other systems read/write this state.
- [ ] AI note: how a non-player country uses the mechanic (even roughly).
- [ ] Edge cases & failure modes; what happens at 0 and at cap.
- [ ] Explicitly listed cuts: what this deliberately does NOT model.
- [ ] Open questions resolved or moved to a "post-slice" section.

## Style

Keep docs short and decisive — a page or two. Prefer "X works like this"
over surveys of options; when a choice is genuinely open, mark it as an
open question with a leaning. Record rejected alternatives in one line each.
