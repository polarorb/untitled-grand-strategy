# Development log

How this game is actually being built — decisions, mistakes, and numbers.
Built in Rust + Bevy with Claude Code driving development and agent-swarm
research workflows; that's part of the story and we don't pretend
otherwise. Newest first.

---

## 2026-08-25 — Famine, weather, and the sound of the war room

**Step 4: agriculture** ([design doc](design/systems/agriculture.md)).
Each January every country rolls a harvest (850–1150‰) from the seeded
RNG — the sim's first real use of randomness, fully deterministic. Planned
economies get the two levers history handed them: **grain procurement
quotas** (squeeze the countryside harder — and when the harvest fails,
the famine deaths concentrate rural) and **collectivization**, priced
honestly as the historical gamble: permanently better extraction,
permanently worse yield, and a 12-month transition shock. Food ratios
now feed back into standard of living and, below the famine threshold,
into excess deaths applied straight to the demographic cohorts. The test
suite's grimmest assertion yet: `famine_is_reachable_and_kills`. The
Great Leap is now a reachable outcome of player choices, not an event.
Soviet food sits at 94% with a normal harvest — chronically strained,
exactly as it should be.

**Audio research + first sounds.** A 3-analyst swarm produced the
[audio readup](research/audio.md) with some sharp verified findings:
1950s recordings are copyright-locked until ~2060 (no Fallout-style
needle-drops), the Conet Project numbers-station tapes are actively
litigated (we'll synthesize our own), and FreePD.com is dead but its CC0
catalog lives on archive.org. The direction doc
([audio](design/systems/audio.md)) commits to DEFCON's bunker
philosophy: teletype as our Geiger counter, stingers followed by
enforced silence, and a stem mixer driven by the Tension meter. v1
shipped: CC0 menu music and Kenney UI clicks — and the war room itself
stays silent on purpose.

## 2026-08-25 — The thesis becomes playable: two planning interfaces

Step 3, and the reason this game exists
([design doc](design/systems/planning-interfaces.md)): every country now
runs one of two economic systems over the same production substrate.
Monthly output — throttled by regional power and coal — splits into
consumer goods, investment, and military. **Planned economies set the
quantities**: quota rows in 5% steps, and their statistics bureau pads
disappointing results (the player's own dashboard shows *reported*
industry, capped at +15% above reality — the espionage pillar will
eventually let rivals know your economy better than you do). **Market
economies set the parameters**: interest, taxes, procurement; firms
decide the split, statistics are honest, and loose money shows up as
inflation that eats living standards.

The loop is closed: allocation → consumer goods → standard of living →
birth rates, urbanization, education. Guns-versus-butter now has
demographic teeth. Press E in-game: Stalin gets STATE PLANNING with
"(figures as reported by Gosplan)"; Truman gets ECONOMIC POLICY with
"firms set output; you set the terms". Same panel skeleton, different
verbs — the first-hour difference the research demanded.

The five new tests read like the design goals: heavier investment grows
industry faster; cheap money boosts growth *and* inflation; planned
statistics pad but stay capped; market statistics never lie; commands
aimed at the wrong interface are rejected. 32 tests green, economy
digest in the determinism suite.

## 2026-08-25 — Resources, deposits, and the unelectrified world

Step 2 of the economy build order
([design doc](design/systems/resources-and-grids.md)): the world now has
**243 economic regions** (spatial clustering of provinces, never crossing
a country border), **51 hand-placed 1950 resource deposits** — Donbas and
Ruhr coal, Ghawar and Baku oil, Shinkolobwe uranium correctly in the
Belgian Congo — and monthly **national balances** for grain, coal, oil,
and steel plus **regional electricity** where shortage softly throttles
(never below 50%, per the research).

The new Power map mode is the payoff: press M twice and the 1950 energy
map appears — the American/European/Soviet industrial cores green, the
entire decolonizing world red. Nobody scripted that; it falls out of
industry distribution and urban population. Grain comes from rural
cohorts × terrain, so the demography system now feeds the economy — the
first real inter-system dependency. Uranium accumulates into national
stockpiles, inert until the weapons-program chain lands.

Amusing bug from nearest-province deposit assignment: Saudi oil initially
went to Bahrain, Katanga's uranium to Northern Rhodesia, and Lorraine's
iron to Luxembourg — all fixed by nudging coordinates, but a good
reminder that "nearest center" and "inside the polygon" are different
questions. 27 tests green, including "the unelectrified world should
exist" (>20 regions at full power AND >20 starved).

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
