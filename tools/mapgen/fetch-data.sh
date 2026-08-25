#!/bin/sh
# Fetch the source geodata for mapgen. Licenses (see docs/research/geodata.md):
#   Natural Earth  — public domain
#   HYDE 3.2.1     — CC0 (public domain) via DANS
#   ETOPO5         — public domain (NOAA)
#   Köppen-Geiger  — CC BY 4.0 (Beck et al. 2023, GloH2O) — attribution
#                    required in game credits.
# Output lands in tools/mapgen/data/ (gitignored).
set -eu
cd "$(dirname "$0")"
mkdir -p data

# Worldwide first-level admin units (~4,600): our province atoms.
if [ ! -f data/ne_10m_admin_1.geojson ]; then
    echo "fetching Natural Earth admin-1..."
    curl -sL -o data/ne_10m_admin_1.geojson \
        "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_10m_admin_1_states_provinces.geojson"
fi

# ETOPO5 global elevation, 5 arcmin, raw big-endian i16.
if [ ! -f data/ETOPO5.DAT ]; then
    echo "fetching ETOPO5..."
    curl -sL -o data/ETOPO5.DAT \
        "https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO5/TOPO/ETOPO5/ETOPO5.DAT"
fi

# Köppen-Geiger 1931-1960 at 0.1° (Beck et al., figshare article 21789074).
if [ ! -f data/1931_1960/koppen_geiger_0p1.tif ]; then
    echo "fetching Koppen-Geiger..."
    curl -sL -o data/koppen_geiger_tif.zip "https://ndownloader.figshare.com/files/61012822"
    unzip -o -q data/koppen_geiger_tif.zip "1931_1960/koppen_geiger_0p1.tif" -d data
    rm data/koppen_geiger_tif.zip
fi

# HYDE 3.2.1 population 1950 (total + urban), 5 arcmin ESRI ASCII.
# The full DANS archive is 5.3 GB; extract just the two 1950 members via
# HTTP Range requests against the zip's central directory.
if [ ! -f data/popc_1950AD.asc ] || [ ! -f data/urbc_1950AD.asc ]; then
    echo "fetching HYDE 3.2.1 1950 population (partial zip extraction)..."
    python3 hyde_fetch.py data
fi

echo "done:"
ls -la data | grep -vE "^total|^\." || true
