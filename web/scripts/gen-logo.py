"""Writes the logo and icon files in public/: logo-dark.svg, logo-light.svg, favicon.svg and, when
`rsvg-convert` is installed, apple-touch-icon.png.

The wordmark is Geist at weight 600, outlined from the variable font in node_modules, so the logo
matches the interface font. Run `python3 scripts/gen-logo.py` after `pnpm install`; it needs the
`fonttools` and `brotli` Python packages.
"""
import shutil
import subprocess
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

root=str(Path(__file__).resolve().parent.parent)+'/'
f=TTFont(root+'node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2')
f=instantiateVariableFont(f,{'wght':600})
upm=f['head'].unitsPerEm
gs=f.getGlyphSet(); cmap=f.getBestCmap(); hmtx=f['hmtx']
word='Wobblescope'
# Per letter: (drop from the baseline, rotation in degrees). The two o letters are orange.
wob=[(-24.0,-12.0),(2.3,1.1),(12.7,6.4),(-3.7,-1.8),(-6.3,-3.1),(3.2,1.6),(2.8,1.4),(-2.3,-1.2),(-1.1,-0.6),(1.5,0.7),(0.3,0.2)]
orange={1,8}
SIZE=96; s=SIZE/upm; TRACK=3; K=1.15
def build(ink, rule_opacity=0.4):
    cur=6.0; parts=[]
    for i,ch in enumerate(word):
        g=cmap[ord(ch)]; adv=hmtx[g][0]
        pen=SVGPathPen(gs, ntos=lambda v: ('%.1f'%v).rstrip('0').rstrip('.'))
        gs[g].draw(pen)
        d=pen.getCommands()
        cx=cur+adv*s/2; dy,rot=wob[i]
        by=100+dy*K
        fill='#FF5A1F' if i in orange else ink
        parts.append(f'    <path transform="translate({cx:.1f} {by:.1f}) rotate({rot:.1f}) translate({-adv*s/2:.2f} 0) scale({s:.4f} {-s:.4f})" fill="{fill}" d="{d}"/>')
        cur+=adv*s+TRACK-(10 if i==0 else 0)
    return parts, cur
ECHO = {"dark": "#8A381C", "light": "#FDBBA4"}


def lockup(ink, outline, outline_width, echo):
    parts, end = build(ink)
    width = round(220 + end + 14)
    mark = f'''  <g transform="scale(0.909091) translate(110 100)">
    <rect x="-92" y="-80" width="160" height="160" rx="38" fill="none" stroke="{echo}" stroke-width="6"/>
    <rect x="-68" y="-80" width="160" height="160" rx="38" fill="none" stroke="{echo}" stroke-width="6"/>
    <rect x="-80" y="-80" width="160" height="160" rx="38" fill="#FF5A1F" stroke="{outline}" stroke-width="{outline_width}"/>
    <circle cx="0" cy="-8" r="32" fill="#F2F4EF" stroke="#10201C" stroke-width="8"/>
    <circle cx="6" cy="-4" r="13" fill="#10201C"/>
    <circle cx="11" cy="-9" r="4" fill="#F2F4EF"/>
    <path d="M-24 46 q12 -14 24 0 t24 0" fill="none" stroke="#10201C" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"/>
  </g>'''
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} 182" width="{width}" height="182" role="img" aria-label="Wobblescope">
{mark}
  <g transform="translate(220 26)">
    <line x1="0" y1="100" x2="{end:.0f}" y2="100" stroke="{ink}" stroke-width="1.5" stroke-dasharray="2 6" stroke-linecap="round" opacity="0.4"/>
{chr(10).join(parts)}
  </g>
</svg>
'''


# The icon is a square tile with the eye above the mouth. At 24 px and below the mouth is a smudge, so a
# media query on the image size drops it and centres a larger eye (browsers that ignore it keep the mouth).
FAVICON = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <style>
    @media (max-width: 24px) {
      .mouth { display: none; }
      .eye { transform-origin: 32px 25px; transform: translateY(7px) scale(1.0714); }
    }
  </style>
  <rect width="64" height="64" rx="15" fill="#FF5A1F"/>
  <g class="eye">
    <circle cx="32" cy="25" r="14" fill="#F2F4EF" stroke="#10201C" stroke-width="3.5"/>
    <circle cx="34.4" cy="26.3" r="6" fill="#10201C"/>
    <circle cx="36.5" cy="23.6" r="2" fill="#F2F4EF"/>
  </g>
  <path class="mouth" d="M22 48 q5 -6 10 0 t10 0" fill="none" stroke="#10201C" stroke-width="3" stroke-linecap="round"/>
</svg>
'''

# iOS rounds the corners itself and fills transparency with black, so the icon is an opaque full-bleed square.
APPLE_TOUCH = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 180 180" width="180" height="180">
  <rect width="180" height="180" fill="#FF5A1F"/>
  <circle cx="90" cy="72" r="40" fill="#F2F4EF" stroke="#10201C" stroke-width="10"/>
  <circle cx="98" cy="77" r="17" fill="#10201C"/>
  <circle cx="105" cy="70" r="5.5" fill="#F2F4EF"/>
  <path d="M62 139 q14 -18 28 0 t28 0" fill="none" stroke="#10201C" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
'''

public = root + "public/"
open(public + "logo-dark.svg", "w").write(lockup("#F2F4EF", "#F2F4EF", 8, ECHO["dark"]))
open(public + "logo-light.svg", "w").write(lockup("#10201C", "#10201C", 6, ECHO["light"]))
open(public + "favicon.svg", "w").write(FAVICON)
if shutil.which("rsvg-convert"):
    subprocess.run(
        ["rsvg-convert", "-w", "180", "-h", "180", "-o", public + "apple-touch-icon.png"],
        input=APPLE_TOUCH.encode(),
        check=True,
    )
else:
    print("rsvg-convert not found: apple-touch-icon.png was not rebuilt")
print("ok", upm)
