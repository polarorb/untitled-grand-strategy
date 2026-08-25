#!/bin/sh
# Fetch the source geodata for mapgen. Everything downloaded here is
# PUBLIC DOMAIN (Natural Earth, naturalearthdata.com) and safe to ship.
# Output lands in tools/mapgen/data/ (gitignored).
set -eu
cd "$(dirname "$0")"
mkdir -p data
base="https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson"
fetch() {
    [ -f "data/$2" ] && { echo "have $2"; return; }
    echo "fetching $2 ..."
    curl -sL -o "data/$2" "$base/$1"
}
# Worldwide first-level admin units (~4,600): our province atoms.
fetch ne_10m_admin_1_states_provinces.geojson ne_10m_admin_1.geojson
echo "done"
