# Geodata sources & licensing

Researched 2026-08-25. Governs what may ship in the game vs. what is
reference-only.

## Shipped in the game (safe for any use, including commercial)

- **Natural Earth** (naturalearthdata.com) — PUBLIC DOMAIN.
  - `ne_10m_admin_1_states_provinces`: ~4,600 worldwide first-level admin
    units. Our **province atoms**: real polygons, names, `adm0_a3` country
    codes, per-unit lat/lon. Fetched as GeoJSON from the official
    `nvkelso/natural-earth-vector` GitHub mirror by
    `tools/mapgen/fetch-data.sh` (gitignored; ~40 MB).
  - 1950 borders are NOT taken from any historical dataset. They emerge
    from **province ownership**, authored by us in
    `tools/mapgen/owners_1950.csv` (modern `adm0_a3` → 1950 sovereign tag,
    plus admin-1-level overrides for divided countries like Germany).
    Ownership data is original research → ours to license.

## Reference-only (do NOT ship — license or quality)

- **CShapes 2.0** (icr.ethz.ch/data/cshapes) — historical state borders
  1886–2019, GeoJSON. License **CC BY-NC-SA 4.0 (non-commercial)**.
  Usable only to visually cross-check our 1950 ownership table.
- **aourednik/historical-basemaps** (GitHub) — world borders by year incl.
  1950. No clear license found; precision self-described as approximate.
  Same status: eyeball-verification only.

## Additional shipped-data sources (integrated 2026-08-25)

- **HYDE 3.2.1** (PBL/Utrecht, via DANS doi:10.17026/dans-25g-gez3) —
  **CC0 (public domain)**. `popc_1950AD.asc` + `urbc_1950AD.asc`, 5 arcmin.
  Source of per-province `population_k` and the urban-share input to
  terrain. The DANS archive is one 5.3 GB zip in Deflate64;
  `tools/mapgen/hyde_fetch.py` extracts just the two 1950 members
  (~10 MB transferred) via HTTP Range requests. Validation: world total
  comes out 2.53B vs. ~2.5B actual 1950; USSR 181M/actual 180M, Japan
  83M/83M, USA 160M/152M.
- **ETOPO5** (NOAA NGDC) — public domain. Global elevation, 5 arcmin, raw
  big-endian i16 (`ETOPO5.DAT`, row 0 at 90°N, col 0 at 0°E). Drives
  Mountain/Hills classification (mean/std elevation per province).
- **Köppen-Geiger 1931–1960** (Beck et al. 2023, figshare 21789074) —
  **CC BY 4.0: attribution required in game credits.** 0.1° GeoTIFF,
  period-correct for a 1950 start. Drives Desert/Jungle/Tundra/Forest.
  Note: palette-color TIFF; mapgen patches photometric to grayscale
  in-memory to read raw class indices.

### Terrain classification rules (v1, in `classify()`)

Urban (density >400/km² and urban share >45%) → Mountain (mean elev
>2000 m or σ >650 m) → Hills (mean >900 m or σ >280 m) → by Köppen
majority: Af/Am jungle, BW desert, ET/EF tundra, subarctic D forest,
temperate/continental forest-vs-plains split at 15 people/km². Marsh
unused pending wetland data.

## Pipeline summary

`tools/mapgen` (offline, never at game runtime):
NE admin-1 GeoJSON + `owners_1950.csv`
→ id assignment (sorted by `adm1_code`, stable given identical inputs)
→ adjacency (shared quantized boundary points, 0.01°, ≥3 shared)
→ geometry simplification (RDP, 0.03° ≈ 3 km)
→ `assets/data/scenario/1950/provinces/world.ron` (4,594 provinces)
→ `assets/data/scenario/1950/countries/generated.ron` (226 countries)
→ `assets/map/world.geo.ron` (2.9 MB polygon rings)

Known v1 limitations (tracked, deliberate):
- Berlin is one province, assigned to GDR — West Berlin needs a special
  mechanic later.
- Hainan generated as PRC; historically Nationalist-held until Apr 1950.
- No sea zones, straits, or canals yet; adjacency is land-only.
- Some countries are heavily over-divided by NE (GBR 232 units, SVN 193);
  a merge pass to target province counts is future work.
- Province `population_k` = 0 and terrain = Plains pending raster stages.
