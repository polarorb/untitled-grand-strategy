"""Generate UI art with nano-banana 2 (gemini-3-pro-image) where no
suitable sourced asset exists. Requires GEMINI_API_KEY in the environment.

Usage:
  python3 generate_art.py menu               # main-menu background
  python3 generate_art.py portrait TAG "Name" "Title, Country 1950"
                                             # painted placeholder portrait

Everything produced here is marked AI-generated in assets/CREDITS.md by
hand — keep that honest.
"""
import base64
import json
import os
import sys
import urllib.request

MODEL = "gemini-3-pro-image"  # nano-banana 2
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")

MENU_PROMPT = """A wide 16:9 painted background for a Cold War grand strategy
game main menu, set January 1950. A dim war-room: a huge frosted-glass world
map wall glowing pale blue-green at center, silhouetted figures in overcoats
and uniforms studying it from below, cigarette smoke curling through
projector light. Muted palette of slate blue, olive drab, and warm amber
lamplight; heavy 1950s oil-painting texture, dramatic chiaroscuro, no text,
no lettering, painterly, restrained, atmospheric."""

ICON_PROMPT = """A small square UI icon for a Cold War strategy game:
{subject}. Flat vintage-print style on a solid very dark slate background
(hex 12161E), the motif drawn in muted brass gold (hex D4B05C) with subtle
off-white accents, centered, bold simple silhouette readable at 40 pixels,
slight 1950s screen-print texture. No text, no border, no frame."""

PORTRAIT_PROMPT = """A dignified head-and-shoulders oil-painted portrait of
{name}, {title}, as they appeared around 1950. Somber official-portrait
style: dark neutral background, formal period dress, warm directional light,
visible brushwork, muted 1950s palette. No text, no frame."""


def generate(prompt, aspect):
    key = os.environ["GEMINI_API_KEY"]
    body = {
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"],
            "imageConfig": {"aspectRatio": aspect},
        },
    }
    req = urllib.request.Request(
        f"https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "x-goog-api-key": key},
    )
    with urllib.request.urlopen(req, timeout=300) as r:
        resp = json.load(r)
    for part in resp["candidates"][0]["content"]["parts"]:
        if "inlineData" in part:
            mime = part["inlineData"].get("mimeType", "image/png")
            ext = "jpg" if "jpeg" in mime else "png"
            return base64.b64decode(part["inlineData"]["data"]), ext
    raise RuntimeError(f"no image in response: {json.dumps(resp)[:500]}")


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "menu"
    if mode == "menu":
        out = os.path.join(ROOT, "assets", "ui", "menu_bg.png")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        data, ext = generate(MENU_PROMPT, "16:9")
        out = out[:-3] + ext
    elif mode == "icon":
        name, subject = sys.argv[2], sys.argv[3]
        out = os.path.join(ROOT, "assets", "ui", f"icon_{name}.png")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        data, ext = generate(ICON_PROMPT.format(subject=subject), "1:1")
        out = out[:-3] + ext
    elif mode == "portrait":
        tag, name, title = sys.argv[2], sys.argv[3], sys.argv[4]
        out = os.path.join(ROOT, "assets", "leaders", f"{tag}.png")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        data, ext = generate(PORTRAIT_PROMPT.format(name=name, title=title), "3:4")
        out = out[:-3] + ext
    else:
        sys.exit(f"unknown mode {mode}")
    with open(out, "wb") as f:
        f.write(data)
    print(f"wrote {out} ({len(data) / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
