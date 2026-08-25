# Terrain

Status: designed (data layer implemented; combat modifiers await the
combat system)
Pillar: serves 1 via the military game — terrain is why Korea's geography
fights back.

## What terrain is

One `Terrain` per province, classified offline by mapgen from elevation
(ETOPO5), climate (Köppen 1931–1960), and population density/urbanization
(HYDE 1950) — rules in `docs/research/geodata.md`, hand overrides in
`tools/mapgen/src/main.rs::terrain_overrides()`. Terrain is static; it
never changes during a campaign.

## Gameplay modifiers (for the combat/movement systems)

Baseline = Plains. Values are starting points for tuning, expressed as the
constants the military system will consume (`ugs-sim` `tuning` module when
combat lands).

| Terrain | Attacker penalty | Movement cost | Attrition/day | Notes |
|---|---|---|---|---|
| Plains | 0% | 1.0 | 0.0 | Armor bonus later (+20% attack) |
| Forest | −20% | 1.25 | 0.1 | Armor penalty later |
| Hills | −25% | 1.4 | 0.1 | Defender entrenches faster |
| Mountain | −50% | 2.0 | 0.3 | Supply throughput halved; Chuncheon, the Taebaeks |
| Desert | −5% | 1.2 | 0.4 | Attrition is the real enemy; supply range shortened |
| Jungle | −35% | 1.8 | 0.5 | Air support effectiveness −50%; Indochina's teeth |
| Urban | −40% | 1.1 | 0.0 | Armor penalty −40%; slow grinding sieges (Seoul will change hands four times) |
| Marsh | −40% | 1.9 | 0.3 | Currently unassigned by classifier (needs wetland data) |
| Tundra | −10% | 1.6 | 0.4 | Attrition scales harder in winter (seasonality system, later) |

Interactions reserved for later systems, recorded so their hooks are
designed-in and not bolted on:

- **Seasonality** multiplies attrition and movement by month — Korean
  winter 1950–51 must hurt (Chosin). Terrain sets the base; season scales.
- **Supply** uses movement cost as its flow-cost weight, so mountains
  throttle offensives without a separate constant.
- **Escalation**: strategic bombing of Urban provinces carries world-
  opinion costs — terrain ties into the political game there, and only
  there.

## Map mode

`M` toggles Political ↔ Terrain in the app; terrain uses an atlas palette
(defined in `ugs-app::terrain_color`). Future map modes (alignment,
supply) hang off the same `MapMode` mechanism.

## Known data limitations (v1, accepted)

- Marsh never assigned — Pripyat, Mesopotamia, the Everglades read as
  Plains/Desert. Needs a wetland raster; revisit if a campaign there
  matters before then.
- Classification is per whole province; a province spanning coast-to-peak
  gets its average. Finer truth arrives only if provinces get smaller.
- Leningrad-area population reads low (HYDE cell/polygon mismatch);
  city-province populations generally are floors, not exact.
