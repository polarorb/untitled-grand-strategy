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

## Candidates for later pipeline stages

- **HYDE 3.2** (PBL Netherlands) — historical population grids including
  1950, open access. Planned source for `population_k` per province
  (currently 0). License: CC BY (verify version on download).
- **Natural Earth physical** (elevation-derived rasters, rivers, lakes) —
  public domain; planned source for terrain classification (currently all
  `Plains`).

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
