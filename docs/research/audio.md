# Audio — research readup

Researched 2026-08-25 (3-analyst swarm: music sources, SFX + Bevy tech,
audio design). Raw reports: [audio-raw.json](audio-raw.json). Licenses
verified on source pages, not from memory.

## Licensing facts that shape everything

- **1950s music is locked.** Under the Music Modernization Act, US sound
  recordings published 1947–1956 are protected for 110 years — a 1950
  record stays copyrighted until ~2060. Fallout-style needle-drops need
  paid master + sync licenses. The loophole: recordings published before
  1926 are now PD (and pre-1931 compositions), so 1920s 78rpm records
  read plausibly as "old records on the wireless" in 1950.
- **The Conet Project (numbers stations) is litigated** — Irdial sued
  Wilco and won a settlement. Never sample it; a numbers station is
  trivially synthesizable (filtered voice reading digit groups + interval
  melody + shortwave DSP) and then 100% ours.
- **Soviet classical is a trap**: Shostakovich/Prokofiev compositions are
  URAA-restored in the US, not PD, regardless of the recording's license.
- **FreePD.com is dead** (closed after 17 years) — its CC0 catalog
  survives at archive.org/details/freepd (~1,025 tracks, no account).
- BBC Sound Effects archive: non-commercial license, unusable despite
  being thematically perfect.

## Approved sources (policy: CC0/US-gov-PD first, CC-BY with credits second)

| Source | License | Use |
|---|---|---|
| archive.org/details/freepd | CC0 | ambient/tension music beds |
| Kenney.nl Interface Sounds + UI Audio | CC0 | UI clicks (150 sounds) |
| Musopen Kickstarter recordings (archive.org) | CC0 | classical (verify per-item elsewhere on Musopen) |
| Kevin MacLeod / incompetech | CC-BY 4.0 (or $30/track no-credit) | jazz noir ("Covert Affair", "Deadly Roulette") |
| Kai Engel (FMA / archive.org) | CC-BY (check per album) | cold neoclassical piano |
| OpenGameArt CC0 collections | CC0 / OGA-BY | crisis stingers, filler |
| US federal civil-defense audio (archive.org) | PD | Duck-and-Cover-era flavor (verify per item) |
| Freesound.org, CC0 filter only | CC0 | teletype, Geiger, shortwave (login needed) |

Every shipped file gets a provenance line (source URL, author, license,
date verified) in `assets/CREDITS.md`.

## Tech (verified)

Bevy 0.19's built-in audio: OGG by default, mp3/flac/wav behind features;
one-shots and loops, global volume — but no crossfading, tweening, or
loop points. **bevy_kira_audio 0.26 supports Bevy 0.19** (version table
in its README) and adds typed channels, tweened fades, and stem sync —
it *replaces* bevy_audio (disable the `audio` default feature when
migrating). Plan: built-in audio for v1 (menu music + clicks); migrate
to kira when the tension-stem mixer lands.

## The direction (see design doc for the full spec)

DEFCON is the reference: the player is inside a bunker hearing the world
second-hand — restraint, ambiguity, clinical quiet for catastrophe.
FTL/Balatro supply the architecture: synced vertical stems (drone →
pulse → strings → pressure) with gains driven by the Tension meter
(smoothed, with hysteresis). Paradox supplies the outer layer: weighted-
shuffle track selection gated by tension band. TNO supplies the stinger
pattern: 6–10 bespoke stingers for irreversible moments, each followed by
mandatory silence. Anti-patterns, proven by Twilight Struggle digital's
reviews: stock-MIDI martial pastiche, looping alarms, scoring everything.
**Teletype is this game's Geiger counter.**
