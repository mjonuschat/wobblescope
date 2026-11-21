"""Writes ../.github/social-preview.png (1280x640), the image GitHub shows when the repository is
shared. It sets the dark logo and a tagline on the dark theme's background. Upload the PNG by hand
under Settings > Social preview; GitHub has no API for it. Run `python3 scripts/gen-social.py`
after `python3 scripts/gen-logo.py`; it needs `rsvg-convert` and the `fonttools` and `brotli`
Python packages.
"""
import re
import subprocess
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen

root = Path(__file__).resolve().parent.parent
W, H = 1280, 640
# The dark theme's --background and --muted-foreground from src/ui-kit/tokens.css, as sRGB.
BG, MUTED = "#0C0D0F", "#9B9DA1"
LINES = ["Multi-accelerometer resonance testing", "for Klipper and Kalico printers"]
TAGLINE_SIZE = 44
LINE_HEIGHT = 62

font = instantiateVariableFont(
    TTFont(root / "node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2"),
    {"wght": 400},
)
upm = font["head"].unitsPerEm
glyphs, cmap, hmtx = font.getGlyphSet(), font.getBestCmap(), font["hmtx"]
scale = TAGLINE_SIZE / upm


def outline(text, y):
    """The text as one centred SVG path, drawn from the font's outlines."""
    advances = [hmtx[cmap[ord(c)]][0] * scale for c in text]
    x = (W - sum(advances)) / 2
    parts = []
    for c, advance in zip(text, advances):
        pen = SVGPathPen(glyphs, ntos=lambda v: ("%.1f" % v).rstrip("0").rstrip("."))
        glyphs[cmap[ord(c)]].draw(pen)
        d = pen.getCommands()
        if d:
            parts.append(f'<path transform="translate({x:.1f} {y}) scale({scale:.5f} {-scale:.5f})" d="{d}"/>')
        x += advance
    return "".join(parts)


logo = (root / "public/logo-dark.svg").read_text()
logo_w, logo_h = map(float, re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)"', logo).groups())
logo_inner = logo[logo.index(">") + 1 : logo.rindex("</svg>")]
logo_scale = 1040 / logo_w
logo_x, logo_y = (W - logo_w * logo_scale) / 2, 130

first_baseline = logo_y + logo_h * logo_scale + 90
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
  <rect width="{W}" height="{H}" fill="{BG}"/>
  <g transform="translate({logo_x:.1f} {logo_y}) scale({logo_scale:.4f})">{logo_inner}</g>
  <g fill="{MUTED}">{outline(LINES[0], round(first_baseline))}{outline(LINES[1], round(first_baseline + LINE_HEIGHT))}</g>
</svg>'''

out = root.parent / ".github/social-preview.png"
out.parent.mkdir(exist_ok=True)
subprocess.run(["rsvg-convert", "-w", str(W), "-h", str(H), "-o", str(out)], input=svg.encode(), check=True)
print(out)
