# 1950-01-01 sovereignty mapping — verification notes

Research date: 2026-08-25. The table itself lives in
`tools/mapgen/owners_1950.csv` (197 ISO rows + Natural Earth-specific
codes). These are the trickiest calls and their sources, plus deliberate
game deviations.

## Verified edge cases

1. **Indonesia (IDN) — independent.** The Netherlands transferred
   sovereignty to the United States of Indonesia on Dec 27, 1949 — five
   days before game start — explicitly excluding West New Guinea (Dutch
   until 1962).
   [United States of Indonesia](https://en.wikipedia.org/wiki/United_States_of_Indonesia),
   [West New Guinea dispute](https://en.wikipedia.org/wiki/West_New_Guinea_dispute)
2. **Vietnam (VNM) — `divided`, owner FRA.** The Élysée Accords (Mar 1949)
   created the State of Vietnam, but France ratified only Jan 29, 1950;
   the DRV held much of the countryside.
   [Élysée Accords](https://en.wikipedia.org/wiki/%C3%89lys%C3%A9e_Accords),
   [FRUS 1950 VI](https://history.state.gov/historicaldocuments/frus1950v06/d463)
3. **Hainan — Nationalist-held at start.** PLA landing Apr 16, 1950,
   complete by May 1. Jan 1950 map should give Hainan to ROC (mapgen TODO:
   admin-1 override for Hainan → ROC).
   [Battle of Hainan Island](https://en.wikipedia.org/wiki/Battle_of_Hainan_Island)
4. **Libya (LBY) — `occupied` (GBR), not a trusteeship.** UNGA Res 289(IV)
   (Nov 1949) set independence for Jan 1952; Britain ran Tripolitania and
   Cyrenaica, France the Fezzan.
   [UN Yearbook 1950](https://cdn.un.org/unyearbook/yun/pdf/1950/1950_385.pdf)
5. **Somalia (SOM) — ITA trusteeship, with caveat.** UN assigned it to
   Italy Nov 1949 but Italian administration began Apr 1, 1950; British
   military administration in charge on Jan 1. British Somaliland (NE code
   SOL) is a separate British protectorate.
   [Trust Territory of Somaliland](https://en.wikipedia.org/wiki/Trust_Territory_of_Somaliland)
6. **Trieste** — Free Territory (est. 1947), Zone A Anglo-American, Zone B
   Yugoslav, until 1954. Not carved out of ITA yet (province-level TODO).
   [Free Territory of Trieste](https://en.wikipedia.org/wiki/Free_Territory_of_Trieste)
7. **India (IND)** — dominion on Jan 1; republic Jan 26, 1950.
8. **Austria (AUT)** — elected government under four-power occupation
   (until 1955); kept as own tag with `occupied` status, unlike Japan (see
   deviations).
9. **Palestine (PSE)** — West Bank Jordanian-held (annexed Apr 1950), Gaza
   under Egyptian administration; mapped to JOR, Gaza split is a
   province-level TODO.
10. **Baltic states (EST/LVA/LTU)** — Soviet republics de facto; annexation
    never recognized by US/UK — kept as SOV with a note, in case a
    non-recognized-annexation mechanic materializes.

## Deliberate game deviations from strict history

- **Japan → JAP** (research said owner USA, as Japan had no sovereign
  government under SCAP). The game keeps Japan a distinct tag flagged
  `occupied` so it exists as an actor and can regain sovereignty in 1952.
- **Berlin** — single province assigned to GDR (West Berlin mechanic TODO).
- **Kosovo** — Natural Earth sometimes emits `-99`/`KOS` for iso_a3; mapgen
  keys on NE's `adm0_a3` (KOS), mapped to YUG.

## Tag legend (non-ISO)

SOV (USSR), PRC (mainland China), ROC (Taiwan), CSK (Czechoslovakia,
covers CZE+SVK), YUG (Yugoslavia — communist but expelled from Cominform
June 1948: its own pole, not Soviet bloc), FRG/GDR (divided Germany),
POR (Portugal), JAP (occupied Japan).
