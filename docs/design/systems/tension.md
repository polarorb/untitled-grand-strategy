# Global Tension

Status: implemented (`crates/ugs-sim/src/tension.rs`; instant changes via `SimCommand::DebugAdjustTension` only, real sources pending)
Pillar: 1 (escalation & nuclear brinkmanship)
Parent: [escalation.md](escalation.md) — this doc specs the Tension meter
only; ladders/rungs remain a sketch there.

## Player-facing

A single global gauge, 0.0–100.0, always visible in the top bar. It answers
"how close is the world to the brink right now?" Tension rises instantly
from provocations (later: shootdowns, blockades, interventions, blown covert
ops) and decays slowly on its own as crises fade from headlines. The gauge
shows a named band; band changes are the moments players feel.

| Band | Range | Meaning (future gating) |
|---|---|---|
| Calm | 0–24.9 | Detente-era options available |
| Wary | 25–49.9 | Normal Cold War posture |
| Crisis | 50–74.9 | Harsh options unlock; accidents possible |
| Brink | 75–100 | Crisis mechanics; readiness pressure |

## State

`GlobalTension` resource — one world value, `i32` in internal tenths
(0..=1000); displayed value = tenths / 10. Integer, not float: tension gates
discrete outcomes, so it must be exactly reproducible and comparable.

- Start value 1950-01-01: **300** (30.0, mid-Wary). Rationale: first Soviet
  bomb (Aug '49) and fall of China (Oct '49) are fresh; Berlin blockade
  resolved but recent.

## Cadence & formulas

Constants live in `tension::tuning`.

- **Instant changes**: only via `SimCommand`s applied in `TickSet::Commands`
  (v1 ships a debug adjust command; real sources arrive with their systems).
  Clamped to [0, 1000].
- **Decay**: daily, in `TickSet::Politics`, only when above the era floor:
  `decay_per_day = BASE_DECAY_PER_DAY (2) + value / DECAY_SCALE_DIVISOR (250)`
  internal tenths, floored at `ERA_FLOOR (150)`. High tension cools faster
  in absolute terms (headlines fade), but from 100.0 it still takes ~5
  months to reach Wary — a big crisis shadows a season.
- **Era floor**: 150 (15.0) constant for now; later driven by structural
  facts (arsenal sizes, active wars, standing alliances).

## Interactions

- Writes: command processing (this doc), future crisis/espionage/military
  systems via commands or their own Politics-stage systems.
- Reads: escalation rungs, AI aggression gating, event availability (all
  future). UI reads band + displayed value.

## AI note

AI countries read the band as a risk multiplier on provocative actions:
Calm/Wary = normal behavior, Crisis = only committed actors escalate,
Brink = everyone seeks off-ramps except scripted crisis logic.

## Edges & cuts

- At 1000: further increases clamp silently (the ladder, not the meter,
  models "past the brink").
- Below floor: allowed (scripted detente could set it); no decay applies,
  and it drifts back up only via events. No natural rise in v1.
- **Cut from v1**: per-dyad tension (one global value only — revisit after
  Korea plays), automatic war pressure, band-change notifications.
