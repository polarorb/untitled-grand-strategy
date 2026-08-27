#!/usr/bin/env python3
"""Integrate timeline-content-swarm output into scenario data.

Reads the swarm's JSON (regions list), writes:
- assets/data/scenario/1950/events/<NN>-<region>.ron (one per region)
- assets/data/scenario/1950/countries/independence.ron (CountryDef list,
  capital names resolved to ProvinceIds against world.ron)

Validation of names/ids/effects happens in the game loader
(ScenarioData::load), exercised by `cargo test -p ugs-data` and the sim
suite; this script only binds capitals and lays files out.

Usage: python3 tools/timeline/integrate.py <swarm.json>
"""
import json
import re
import sys
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
SCEN = os.path.join(ROOT, "assets", "data", "scenario", "1950")


def load_provinces():
    """name -> list of (id, owner) from world.ron."""
    text = open(os.path.join(SCEN, "provinces", "world.ron")).read()
    out = {}
    # Entries look like: id: (123), name: "X", owner: ("YYY"),
    for m in re.finditer(
        r'id:\s*\((\d+)\),\s*name:\s*"([^"]+)",\s*owner:\s*\("([A-Z]{3})"\)',
        text,
        re.S,
    ):
        pid, name, owner = int(m.group(1)), m.group(2), m.group(3)
        out.setdefault(name, []).append((pid, owner))
    return out


def existing_tags():
    text = open(os.path.join(SCEN, "countries", "generated.ron")).read()
    return set(re.findall(r'tag:\s*\("([A-Z]{3})"\)', text))


def main():
    data = json.load(open(sys.argv[1]))
    regions = data["regions"] if "regions" in data else data
    provinces = load_provinces()
    tags = existing_tags()
    os.makedirs(os.path.join(SCEN, "events"), exist_ok=True)

    new_countries = {}
    problems = []
    for i, region in enumerate(regions):
        key = region["region"].lower().replace(" ", "-")[:24]
        key = re.sub(r"[^a-z0-9-]", "", key)
        ron = region["events_ron"].strip()
        if not (ron.startswith("[") and ron.endswith("]")):
            problems.append(f"{key}: events_ron is not a RON list")
            continue
        path = os.path.join(SCEN, "events", f"{i:02d}-{key}.ron")
        header = "// " + region["region"] + " — timeline content, sourced per event.\n"
        header += "// Sources: " + "; ".join(region.get("sources", []))[:2000] + "\n"
        open(path, "w").write(header + ron + "\n")
        print(f"wrote {path}")

        for nc in region.get("new_countries", []):
            tag = nc["tag"]
            if tag in tags:
                problems.append(f"{key}: tag {tag} collides with an existing country")
                continue
            if tag in new_countries:
                # First writer wins; flag duplicates across regions.
                problems.append(f"{key}: tag {tag} defined twice (kept first)")
                continue
            cands = provinces.get(nc["capital_province"], [])
            match = [pid for pid, owner in cands if owner == nc.get("parent")]
            if not match:
                problems.append(
                    f"{key}: {tag} capital '{nc['capital_province']}' not found under {nc.get('parent')} (candidates: {cands})"
                )
                continue
            nc["capital_id"] = match[0]
            new_countries[tag] = nc

    if new_countries:
        lines = ["// New states 1951-1970, born by Independence events.",
                 "// Dormant until their event fires (no regions owned).", "["]
        for tag, nc in sorted(new_countries.items()):
            r, g, b = nc["color_rgb"]
            lines.append(f"""    CountryDef(
        tag: ("{tag}"),
        name: "{nc['name']}",
        alignment: {nc['alignment']},
        color: ({r}, {g}, {b}),
        capital: ({nc['capital_id']}),
        stability: {nc['stability']},
        industry: {nc['industry']},
        nuclear_power: false,
    ),""")
        lines.append("]")
        path = os.path.join(SCEN, "countries", "independence.ron")
        open(path, "w").write("\n".join(lines) + "\n")
        print(f"wrote {path} ({len(new_countries)} new states)")

    if problems:
        print("\nPROBLEMS:")
        for p in problems:
            print(" -", p)
    print(f"\n{len(regions)} regions, {len(new_countries)} new countries bound.")


if __name__ == "__main__":
    main()
