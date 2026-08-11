"""Regenerate the embedded UI icon bitmaps from the preserved emoji font."""

from __future__ import annotations

import io
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
FONT_PATH = ROOT / "rust-lvgl-panel" / "assets" / "emoji.ttf"
OUTPUT_DIR = ROOT / "rust-lvgl-panel" / "assets" / "emoji"

# The source strike is 109 ppem. These canvas sizes reproduce the bitmap scale
# used by the original Cog UI's headers, block headings, and buttons.
ASSETS = {
    "overview": ("u1F4CA", (50, 47)),
    "settings": ("uni2699", (50, 47)),
    "network": ("u1F310", (35, 33)),
    "storage": ("u1F4BE", (35, 33)),
    "core": ("u1F6E0", (35, 33)),
    "docker": ("u1F433", (35, 33)),
    "monitor": ("u1F5A5", (35, 33)),
    "warning": ("uni26A0", (35, 33)),
    "restart": ("u1F504", (45, 42)),
}


def main() -> None:
    font = TTFont(FONT_PATH)
    strike = font["CBDT"].strikeData[0]
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    for asset_name, (glyph_name, size) in ASSETS.items():
        data = strike[glyph_name].data
        png_offset = data.index(b"\x89PNG\r\n\x1a\n")
        image = Image.open(io.BytesIO(data[png_offset:])).convert("RGBA")
        image = image.resize(size, Image.Resampling.LANCZOS)
        output = OUTPUT_DIR / f"{asset_name}-{size[0]}x{size[1]}.rgba"
        output.write_bytes(image.tobytes())
        print(f"{output.relative_to(ROOT)}: {size[0]}x{size[1]}")


if __name__ == "__main__":
    main()
