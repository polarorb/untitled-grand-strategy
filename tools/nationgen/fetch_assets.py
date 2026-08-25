"""Fetch nation-select assets (1950 flags, leader portraits) from Wikimedia
Commons, given the research JSON produced by the nation-research workflow.

Usage: python3 fetch_assets.py [research.json]

- Flags   -> assets/flags/<TAG>.png   (640px thumbnail; SVGs rasterize)
- Leaders -> assets/leaders/<TAG>.<ext> (512px thumbnail)
- License lines appended to assets/CREDITS.md (rewritten each run)

Files that fail to resolve are reported at the end so the research data
can be corrected; nothing is silently skipped.
"""
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
API = "https://commons.wikimedia.org/w/api.php"
UA = "untitled-grand-strategy/0.1 (game asset pipeline; erik.rahtjen@gmail.com)"


def api_query(titles, width):
    params = urllib.parse.urlencode({
        "action": "query",
        "titles": "|".join(titles),
        "prop": "imageinfo",
        "iiprop": "url|extmetadata",
        "iiurlwidth": str(width),
        "redirects": "1",
        "format": "json",
    })
    req = urllib.request.Request(f"{API}?{params}", headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def fetch(url, dest):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as r:
        data = r.read()
    with open(dest, "wb") as f:
        f.write(data)
    return len(data)


def resolve_and_download(jobs, width, out_dir, ext_override=None):
    """jobs: list of (tag, commons_title). Returns (credits, failures)."""
    os.makedirs(out_dir, exist_ok=True)
    credits, failures = [], []
    import glob as _glob
    for tag, title in jobs:
        if not title:
            failures.append((tag, "(no file given)"))
            continue
        already = bool(_glob.glob(os.path.join(out_dir, f"{tag}.*")))
        if not title.startswith("File:"):
            title = "File:" + title
        try:
            data = None
            for attempt in range(4):
                try:
                    data = api_query([title], width)
                    break
                except urllib.error.HTTPError as e:
                    if e.code == 429 and attempt < 3:
                        time.sleep(45)
                    else:
                        raise
            pages = data["query"]["pages"]
            page = next(iter(pages.values()))
            if "imageinfo" not in page:
                failures.append((tag, f"{title} — not found"))
                continue
            info = page["imageinfo"][0]
            thumb = info.get("thumburl") or info["url"]
            ext = ext_override or os.path.splitext(urllib.parse.urlparse(thumb).path)[1].lstrip(".").lower() or "png"
            dest = os.path.join(out_dir, f"{tag}.{ext}")
            if not already:
                fetch(thumb, dest)
            meta = info.get("extmetadata", {})
            license_short = meta.get("LicenseShortName", {}).get("value", "unknown")
            artist = meta.get("Artist", {}).get("value", "")
            # crude de-HTML for credits
            import re
            artist = re.sub(r"<[^>]+>", "", artist).strip()
            credits.append(
                f"- `{os.path.relpath(dest, ROOT)}` — [{page['title']}]"
                f"(https://commons.wikimedia.org/wiki/{urllib.parse.quote(page['title'].replace(' ', '_'))})"
                f" — {license_short}" + (f" — {artist}" if artist else "")
            )
            print(f"  {tag}: {page['title']} ({license_short})")
        except Exception as e:  # noqa: BLE001 - report and continue
            failures.append((tag, f"{title} — {e}"))
        time.sleep(2.0)  # be polite to the API (429s at higher rates)
    return credits, failures


def main():
    research_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "research.json")
    nations = json.load(open(research_path))["nations"]
    print(f"{len(nations)} nations")

    print("flags:")
    flag_credits, flag_fail = resolve_and_download(
        [(n["tag"], n.get("flag_commons_file")) for n in nations],
        width=640, out_dir=os.path.join(ROOT, "assets", "flags"))
    print("leaders:")
    leader_credits, leader_fail = resolve_and_download(
        [(n["tag"], n.get("leader_commons_file")) for n in nations],
        width=512, out_dir=os.path.join(ROOT, "assets", "leaders"))

    ledger_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "credits.json")
    ledger = json.load(open(ledger_path)) if os.path.exists(ledger_path) else {"flags": [], "leaders": []}
    ledger["flags"] = sorted(set(ledger["flags"]) | set(flag_credits))
    ledger["leaders"] = sorted(set(ledger["leaders"]) | set(leader_credits))
    json.dump(ledger, open(ledger_path, "w"), indent=1)
    flag_credits, leader_credits = ledger["flags"], ledger["leaders"]

    credits_path = os.path.join(ROOT, "assets", "CREDITS.md")
    with open(credits_path, "w") as f:
        f.write("# Asset credits\n\n")
        f.write("Climate classification: Beck, H.E. et al. (2023), Koppen-Geiger\n")
        f.write("maps (CC BY 4.0), https://www.gloh2o.org/koppen/\n\n")
        f.write("## Flags (Wikimedia Commons)\n\n")
        f.write("\n".join(sorted(flag_credits)) + "\n\n")
        f.write("## Leader portraits (Wikimedia Commons)\n\n")
        f.write("\n".join(sorted(leader_credits)) + "\n")
    print(f"wrote {credits_path}")

    if flag_fail or leader_fail:
        print("\nFAILURES (fix research data and re-run):")
        for tag, why in flag_fail:
            print(f"  flag  {tag}: {why}")
        for tag, why in leader_fail:
            print(f"  leader {tag}: {why}")
    print(f"\nok: {len(flag_credits)} flags, {len(leader_credits)} portraits; "
          f"failed: {len(flag_fail)} flags, {len(leader_fail)} portraits")


if __name__ == "__main__":
    main()
