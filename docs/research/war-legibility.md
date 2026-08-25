# War legibility: making a war readable while you watch it

*Research swarm, 2026-08-25 — four analysts on map-scale force display,
battle inspection UI, war-level dashboards & intelligence estimates, and
the manpower pipeline. Raw findings in
[war-legibility-raw.json](war-legibility-raw.json).*

The prompt, verbatim from the director: *"I want way more info when
watching a war — troops per province, overall army numbers, where troops
come from (seems like magic!), why I am losing, estimates of enemy
losses."* This is the exact complaint the earlier military research
predicted ("an after-action/'why' layer is first-class UI, not polish").
The four analysts converged hard; the synthesis below is what we build.

## The convergent principles

**1. Counters carry four channels, and bars beat numbers at map scale.**
The NATO counter tradition (affiliation color, type glyph, size
designator, strength figures) survives because it reads at 24px in
grayscale. HoI4's single best element is the dual bar on every stack —
horizontal **strength** (dies slowly) and vertical **organization**
(breaks first) — which is one-to-one with our cohesion/strength model.
Community counter mods exist almost entirely to make those bars bigger.
Unity of Command's beloved chips go further: strength as *countable
objects* (pips), because humans subitize ~5-7 items instantly.

**2. Battles must exist as on-map objects.** HoI4 spawns a
crossed-swords bubble with a win/lose gauge for every combat; a paused
player sweeps the front and triages. Without a marker, hourly combat is
invisible until provinces flip — literally the "magic" complaint.

**3. "Why am I losing" = a visible, signed, sorted modifier ledger plus
one generated sentence.** The universal criticism of Paradox combat
windows is that the "why" hides in stacked hover tooltips; Vic3 is the
canonical failure (outcomes with no visible inputs — battles at parity
while you outnumber 3:1). The fix nobody ships: print the top modifiers
per side *inline*, then auto-diagnose — "You are losing primarily
because of the mountain defense (+60%)." Compute the largest modifier
gap, template a sentence. ~50 lines.

**4. The single highest-value number is a break-time projection.**
"Defender breaks in ~9h at current rate" — trivial linear extrapolation
of the hourly cohesion delta the sim already produces. Unity of
Command's dev learned to forecast in *concrete units* (losses, time,
outcome buckets), never abstract odds ratios.

**5. Enemy numbers are always ranges, never exact — and that's period
flavor, not friction.** HoI4's intel tiers (±80% at zero intel narrowing
to exact), War in the Pacific's claimed-vs-confirmed losses (pilots
overclaim; you count wrecks only where you held the field), Twilight
Struggle's insight that *strategic* aggregates were roughly known to
both sides while *operational* detail was murky. Rules: round estimates
to two significant figures (false precision destroys the fiction);
sample the fuzz once per period so numbers don't jitter; own losses are
always exact.

**6. Troops stop being magic when the UI shows conservation of mass.**
Victoria 2's soldier-POPs remain the genre's gold standard: regiments
raised from a named province's real population, visibly unable to refill
when it's bled dry. HoI4's pipeline (population × conscription law →
pool → 10%/day reinforcement into divisions) works but fails silently
when the pool empties — its players' #1 confusion. The display answer: a
pipeline strip where every man is in exactly one bucket — POPULATION →
RESERVE → FIELDED → DEAD — with the buckets visibly summing.

**7. Wars feel trackable through a small fixed set of aggregates on a
human timescale.** Casualties both sides, net provinces, battles W/L,
manpower committed vs available, and — absent from every Paradox title
but the most-requested texture in AAR culture — *front tempo*: "front
static 23 days."

**8. A war needs a narrative memory.** A timestamped ticker (battle
begun, division destroyed, province taken) is where "losing" becomes a
story the player can read backwards. Wire-service voice fits our
teletype identity exactly.

## What we implemented (same day)

- **Manpower pipeline in the sim**: pools seeded at 1.5% of real
  populations, wartime mobilization adds 0.2%/month, resting formations
  reinforce 15 strength/day *from the pool* (1 strength = 10 men).
  Fielded men, reserve, and war dead now reconcile against demography.
- **Counters**: nation-color boxes with exact division count + men +
  strength bar (bottom, green/amber/red) + cohesion bar (left, cyan)
  for the player; enemies show a fuzzed count band ("2-4?"), dimmed, no
  bars.
- **Pulsing battle markers** at every contested province.
- **Battle inspector** on selecting a contested province: balance-of-
  power bar, both sides' divisions/men/cohesion with per-hour deltas,
  inline modifier ledger, "X BREAKS IN ~NH" projection, and the one-line
  diagnosis.
- **War room** (R): manpower pipeline strip, battles W/L, front-static
  days, per-enemy strength & losses as monthly-resampled 2-sig-fig
  ranges (own losses exact), and THE WIRE — the last 8 ticker lines.
- **HUD**: standing "ARMY 210k / RESERVE 480k" headline.
- **Division identity & provenance** (second pass, same day): every
  division is raised from a named home province ("3RD SEOUL INFANTRY"),
  expeditionary forces from their nation's most populous province, and
  war dead debit the home province's actual cohorts — rural first.
  Divisions in transit trail movement arrows; the battle inspector lists
  your divisions by name.
- **War momentum**: a decomposed -100..+100 tug-of-war bar per war
  (ground ±40, casualty exchange ±30, run of battles ±15 — every term
  printed) with a generated assessment line and a reserve-low warning.

## Deferred (validated by research, not yet built)

- Pre-battle forecast card before committing to an attack.
- Intel as a real 0-100 per-country stat (grown by front contact/recon,
  decayed by time) driving estimate width; sighting staleness with
  ghost markers for off-front enemy stacks.
- Cumulative dual-line casualty chart; front strength ribbon.
- Aggregated 30-day "assessment" box naming systemic causes across
  battles.
