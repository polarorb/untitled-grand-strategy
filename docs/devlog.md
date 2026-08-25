# Development log

How this game is actually being built — decisions, mistakes, and numbers.
Built in Rust + Bevy with Claude Code driving development and agent-swarm
research workflows; that's part of the story and we don't pretend
otherwise. Newest first.

---

## 2026-08-25 — First mechanics: demography lands

The economy research locked "population is the universal denominator" as
principle #3, so demography is the first real mechanic:
[design doc](design/systems/demography.md). Every province now carries
three continuous cohorts (rural / urban / educated) in integer persons,
seeded from the HYDE census data — we extended the mapgen pipeline to
carry urban counts through (New York: 9.5M urban of 14.6M; Seoul: 844k of
2.9M). Monthly, each province applies birth/death rates driven by a
standard-of-living score, plus rural→urban migration and an education
conversion.

The satisfying part: the calibration test runs ten game-years (87,600
hourly ticks) and asserts world population growth lands in the historical
1950→1960 band (2.53B → ~3.0B) — and it does, first try, from two linear
vital-rate formulas. Death rates came out eerily close to history (USA
model 9.4 vs actual 9.6 per 1000; India 28 vs 28). Birth rates are the
honest weak spot: the Western baby boom defies any SoL curve, so the doc
carries a per-country-override TODO.

All integer math, `BTreeMap` iteration only, and the cohort digest is now
part of the interleaved determinism test. 24 tests green.

## 2026-08-25 — What the genre taught us about economies

Before implementing any economy, we ran an 8-agent research swarm across
HoI4, Victoria 2/3, EU4, Supreme Ruler, Workers & Resources, the Cold War
niche games, Civ, and Stellaris plus design literature. The analysts
worked independently and converged on the same principles — the readup
([economy-mechanics](research/economy-mechanics.md)) distills ten of
them. The big one: planned economies set *quantities* while market
economies set *parameters*, over one shared stock-flow simulation. Also:
never let a currency purchase itself (HoI4's civ-snowball), never let
electricity be money (Stellaris), and model the bomb as a supply chain
with an intelligence footprint, not a research slider. The
[economy design doc](design/systems/economy.md) is rewritten around this
architecture.

## 2026-08-25 — Map-first nation select, fonts, and a proper HUD

The 86-row country list died; the world map is now the country browser.
Click any province → its owner's dossier pops over the map (period flag,
leader photo, government, typewriter-set situation text, PLAY). Typeface
research settled on Oswald / Jost / Courier Prime (all SIL OFL) — Jost
because it's the Futura lineage, and Futura *is* mid-century modernism.
In-game there's now a real top bar (your flag, leader, date, tension) and
a province card with live cohort numbers.

Lesson learned the hard way: Bevy's initial-state `OnEnter` runs before
`Startup` systems, which bit us three separate times (world data, camera,
player nation). Everything a screen needs at first paint now gets
inserted at app-build time.

## 2026-08-25 — Map polish: one mesh, real borders

The map was 13,782 mesh entities and it showed at overview zoom. Now the
whole political fill is a single vertex-colored mesh (9 drawn entities
including borders, across the 3 wrap copies); map modes recolor by
rewriting a province's slice of the color attribute. Country borders are
extracted in mapgen from the raw Natural Earth shared-edge topology —
segments touched by two different owners, chained through junctions into
250 polylines — so the Yalu and the 38th parallel read as real frontiers.
Since border lines are world-unit geometry, they fade at overview zoom
and sharpen close-in: free level-of-detail.

## 2026-08-25 — 86 nations, researched and playable

An 8-agent swarm wrote dossiers for every 1950 nation: de-facto leader on
January 1st exactly (Syria mid-coup-cycle was the hard one), period
government labels, and two paragraphs of situation text each. The asset
pipeline pulled 86 period-correct flags and 84 leader photos from
Wikimedia Commons with licenses recorded; nano-banana 2 painted the
war-room menu background and the two portraits history didn't leave us
(Andorra, San Marino). One deliberate deviation from strict history:
occupied Japan keeps its own tag rather than being owner=USA, so it can
exist as an actor. Full sourcing in
[sovereignty-1950](research/sovereignty-1950.md).

## 2026-08-25 — A world from open data

4,594 provinces from Natural Earth admin-1 (public domain), 1950
ownership researched country-by-country (86 sovereign entities — the
British Empire alone is 923 provinces), population from HYDE 3.2.1
(world total: 2.53B modeled vs ~2.5B actual; USSR 181M vs 180M; Japan
83M vs 83M), terrain classified from ETOPO5 elevation + Köppen-Geiger
1931–1960 climate + urban density. Favorite trick: HYDE's archive is one
5.3GB Deflate64 zip, so the fetch script reads the zip's central
directory over HTTP Range requests and extracts just the two 1950 grids —
10MB transferred. CShapes (the academic historical-borders dataset)
turned out to be CC BY-NC, so we ship none of it: 1950 borders emerge
from our own ownership layer over public-domain geometry.

## 2026-08-25 — Foundations

Cargo workspace with a hard wall between `ugs-sim` (deterministic,
headless, bevy_ecs only) and `ugs-app` (Bevy presentation). One tick =
one in-game hour; all randomness through a seeded PCG32 with forkable
per-subsystem streams; saves will be seed + command log. The determinism
test interleaves two apps tick-by-tick for 90 game-days and asserts
bit-identical state — interleaved specifically to catch global-state
leaks that sequential runs would miss. First mechanics through the
pipeline: the Global Tension meter and the command queue that is the
only doorway into sim state.
