# The Monthly Paper

Status: implemented (v1)
Pillars: 3 primarily (the paper shows the world *as you believe it* —
epistemics as interface), 4 (the paper itself is bloc-flavored: a
party organ prints the reported figures, a broadsheet prints honest
but lagged ones), and it is the legibility spine for 1 and 2.
Research: rests on the war-legibility and economic-agency readups
(date-stamped monthly display, no per-tick jitter, every number
decomposable); no new research pass needed — this is presentation
over existing state.

A full-screen period newspaper summarizing the last game month,
toggled with **N**. It is deliberately built as a *section registry*
so it grows into the game's ledger/stats surface: future pages
(arsenal estimates, influence standings, demography) slot in as
sections without layout rework.

## Player-facing description

- Press **N** (or the HUD hint after a month rolls over): the map
  dims under a full-screen aged-newsprint page.
- **The masthead is yours.** Market/Western players read THE
  INTERNATIONAL HERALD in blackletter; planned-economy players read
  the party organ THE PEOPLE'S OBSERVER in bold red gothic sans;
  non-aligned players read THE NATIONAL GAZETTE. Dateline: "VOL. IV,
  No. 7 — JULY 1953 EDITION", price "10¢" / "5 KOP." per bloc.
- **Lead stories**: up to four events that fired in the closed month,
  headline + wire body, in column layout. World events are public
  history — unfiltered.
- **THE WAR REPORT** (only while at war): battles won/lost this
  month, own casualties exact, enemy casualties and strengths as
  intel-banded estimates (existing fuzz machinery), fronts moved or
  static, settlements/armistices signed.
- **COMMERCE & INDUSTRY**: the top economic wire lines (constraint
  onsets, project milestones) plus the national figures — REPORTED
  for the planner's own paper ("PLAN FULFILLED 103%"), true-but-
  lagged for the market press.
- **THE WORLD IN NUMBERS** (right rail, the ledger seed): global
  tension (band + delta over the month), wars in progress, nations
  born this month (from independence events), treaties signed,
  legitimacy standing, arsenal *estimates* of the rival (2
  significant figures).
- Footer: "ALL FIGURES AS OF {date}. FOREIGN FIGURES ARE ESTIMATES."

## State & data flow (presentation-only — no new sim state)

Everything renders from existing resources with a month window:
- Events: `FiredEvents.fired_ticks` filtered to the closed month;
  titles/bodies from `ScenarioData.events`. Births detected by the
  fired event's `Independence` effect (country name from data).
- Wars/treaties: `Military.war_started`, `Settlements.treaties/
  frozen` tick fields.
- Battles/casualties deltas: UI `Local` snapshot of the cumulative
  counters, diffed at month rollover (display-only state, explicitly
  allowed to live UI-side).
- Econ: `RegionSnapshots.wire` + `as_of`; reported vs actual via the
  existing `dashboard_industry_centi` and snapshot columns.
- Epistemics: the shared estimate helpers (`intel_width`,
  `est_men_range`, `fmt_men`, display-side `mix`) move from `war_ui`
  into a `pub(crate) estimates` module used by war room, dossier, and
  paper alike.
- The paper rebuilds when opened and at month rollover; contents are
  frozen for the month (date-stamped, still — the house rule).

## Assets

- Masthead face: an SIL-OFL blackletter (UnifrakturMaguntia) fetched
  as static TTF per the fonts convention; registered in `Fonts` as
  `masthead`. The party-organ masthead uses Oswald bold in red — two
  mastheads, one new font.
- Background: one generated aged-newsprint texture (nano-banana,
  credited AI-generated), tiled/stretched under a translucent paper
  tone so text contrast stays controlled.

## Layout

Fixed full-screen flex: masthead row (rule lines above/below),
three-column body — lead column (55%): stories; right rail (25%):
THE WORLD IN NUMBERS then THE WAR REPORT; left rail (20%): COMMERCE &
INDUSTRY. Content caps per section (4 stories, 6 numbers, 5 econ
lines) so the page always fits without scrolling; a "MORE ON THE
WIRES" count line notes overflow. All body text Courier Prime;
headlines Oswald; masthead per bloc.

## Section registry (the ledger future)

`paper_ui` exposes sections as functions `fn section_x(parent, ctx)`
listed in one array — adding the arsenal page or influence standings
later is appending a function. Planned follow-ons: STRATEGIC BALANCE
(deterrence estimates), THE COLONIAL QUESTION (decolonization
tracker), OBITUARIES (leader deaths from events).

## Edge cases

- Month one (no closed month yet): the paper prints a founding
  edition with the scenario briefing line and current numbers.
- No events fired in the month: lead column falls back to the top
  wire notices; "A QUIET MONTH" filler headline.
- Observer (no player nation): the Gazette masthead, no war report.
- Paper open across a rollover: contents swap at the boundary
  (acceptable; the dateline says which edition you read).

## Deliberately not modeled

Per-country localized papers beyond the three bloc flavors; scrolling
archives of past editions (the current edition only — archives later
with the section registry); photographs; clickable stories (v1 is a
document, not a menu); auto-pause on publication.
