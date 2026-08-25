# Audio

Status: designed (direction locked); v1 implemented = menu music + UI
clicks
Research: [audio readup](../../research/audio.md)

## Identity

The player is **inside the war room**, hearing the world second-hand.
Everything diegetic is filtered as if through concrete and ventilation.
The most terrible events get the smallest sounds. Near-silence is not the
absence of audio design — it is the design, and it is what makes Brink
audible. A blindfolded player should estimate the Tension band from
sound alone.

## The map, by tension band

- **Calm (0–39)**: room tone (ventilation, faint teletype bursts every
  30–90s), music layer A only — slow drone with treated piano. Orders
  sound analog: relay clicks, typewriter strikes, paper slides.
- **Crisis (40–74)**: the room wakes. Teletype frequency doubles; "flash
  traffic" long bursts become their own dread signal. Layers B (clock
  pulse) and C (dissonant strings) fade in on the tension curve. A phone
  rings once, unanswered, at threshold crossings.
- **Brink (75–100)**: inversion — strip the music back. Bare pulse,
  sub-bass pressure, our own synthesized numbers station bleeding
  through shortwave, one high sustained string. One distant, unconfirmed
  human sound, rolled rarely, never looped. The klaxon plays ONCE on
  entry, then oppressive quiet. Geiger clicks tied to nothing at 95+.
- **Pause**: don't stop audio — low-pass and duck ~6dB. Pausing feels
  like holding your breath.

## Architecture (target)

- Synced vertical stems per era-piece (A drone / B pulse / C strings /
  D pressure), gains driven by smoothed Tension with hysteresis so
  boundary jitter can't flap the mix.
- Weighted-shuffle outer layer picks which composition plays, gated by
  tension band + era; tracks never restart on state change — stems
  thicken.
- 6–10 bespoke **stingers** for irreversible moments (first Soviet test
  is already history; first thermonuclear, superpower war, first use,
  Brink entry, armistice), each followed by 10–20s of enforced silence.
- Buses: music / ambience / UI / stinger with ducking sidechains, via
  **bevy_kira_audio** (0.26, replaces bevy_audio when we migrate).
- Compose a 3–5 note identity motif first: it seeds the menu theme, the
  stingers, and the numbers-station interval melody.

## Sound-or-silence policy (every feature must argue for a sound)

Gets a sound: player orders (typewriter/relay), incoming events (teletype
graded by length), tension threshold crossings, year rollover, stingers,
high-consequence UI. Stays silent: unit movement, routine AI actions,
most notifications (badge silently), combat at map zoom (distant rumble
at most — the deaths are numbers, and that should be uncomfortable).

## Era touches (all synthesizable or PD)

Teletype as the primary information sound; shortwave tuning sweeps on
map-mode/intel switches; original numbers stations (espionage + Brink);
CONELRAD-pastiche civil-defense copy (our own words) in nuclear crises;
rotary/switchboard sounds in diplomacy; WWV time pips at high speed;
vacuum-tube warm-up on the intel panel. No full VO — band-limited
fragmentary radio voices only.

## v1 (implemented)

Menu music: "Dark Ambient" (FreePD catalog, CC0) looping over
MainMenu/NationSelect at 0.4 volume; stops on campaign start — the war
room opens silent, which is both the placeholder and the point. UI click
(Kenney Interface Sounds, CC0) on every button press. Built-in
bevy_audio (mp3 + ogg); kira migration deferred to the stem mixer.

## Banned

Fanfares, marching snares, faux-Soviet choirs, looping alarms, 1950s
needle-drops (locked until ~2060), Conet Project samples (litigated),
Soviet classical compositions (URAA-restored), any NC/SA-licensed asset.
