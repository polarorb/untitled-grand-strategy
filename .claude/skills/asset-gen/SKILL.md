---
name: asset-gen
description: Source or generate game assets — flags, portraits, backgrounds, fonts, UI art — with correct licensing and credits. Use whenever the game needs a new image or font asset.
---

# Asset generation

Two pipelines. Always prefer a sourced real asset when one exists;
generate only what cannot be sourced. Every shipped asset gets a credits
line — no exceptions.

## 1. Sourced from Wikimedia Commons

For flags, historical photos, portraits, maps:

1. Find the EXACT Commons file title (`File:...`), period-correct (1950
   flags often differ from modern ones — verify the variant's date range).
   Delegate historical verification to the `historian` agent for anything
   uncertain.
2. Add to the relevant research JSON / asset list and run
   `python3 tools/nationgen/fetch_assets.py` — it resolves titles via the
   Commons API (rate-limit aware: 2s spacing, 429 backoff; skips files
   already on disk), downloads rasterized thumbnails (SVG→PNG), and
   regenerates `assets/CREDITS.md` from the persistent ledger
   (`tools/nationgen/credits.json`).
3. Check the license column it prints. PD/CC0 ship freely; CC BY needs
   the attribution already recorded; ShareAlike-licensed files are
   flagged in CREDITS.md for review before commercial distribution.

## 2. Generated with nano-banana 2

For art with no real-world source (menu backgrounds, missing portraits,
UI textures): `python3 tools/nationgen/generate_art.py <mode> ...` using
model `gemini-3-pro-image` via `GEMINI_API_KEY` (present in the user's
shell env — never echo it). Modes: `menu` (16:9 background),
`portrait TAG "Name" "Title"` (3:4 painted portrait). Add new prompt
modes to the script rather than one-off scripts.

Rules:
- House style: painted, muted 1950s palette, no text in image (prompts in
  the script encode this).
- The API returns JPEG or PNG regardless of requested name — the script
  names by actual MIME type; Bevy needs the `jpeg` feature (already on).
- Every generated asset is listed under "AI-generated assets" in
  `assets/CREDITS.md` with what it depicts and why it was generated
  (e.g. "no Commons photo available").
- Never generate imagery that would be deceptive about a real person;
  painted-portrait style for historical figures is the accepted form.

## 3. Fonts

SIL OFL faces only. Fetch static TTFs via the Google Fonts CSS API
(request `css2?family=...` and download the `fonts.gstatic.com` URLs —
gives static instances rather than variable fonts, which is what Bevy
wants). Register in `Fonts` (crates/ugs-app/src/main.rs), credit in
CREDITS.md. Current roles: Oswald = display, Jost = UI body,
Courier Prime = dossier/typewriter.

## Checklist before finishing

- [ ] Asset renders in-game (or on the Pages site) — verified visually
      via `UGS_SHOT`.
- [ ] `assets/CREDITS.md` regenerated/updated.
- [ ] No desktop screenshots were taken (game self-capture only).
