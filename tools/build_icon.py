# Rebuilds the app icon in assets/icon/ (a one-off asset step, not part of the cargo build):
#   pip install cairosvg pillow
#   python tools/build_icon.py assets/icon
# 16 to 32 px are pixel-snapped by hand (SMALL); 40 px and up are rendered from MASTER.
import cairosvg, io, os, sys
from PIL import Image

OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/icon"
os.makedirs(f"{OUT}/png", exist_ok=True)
BG, FG = "#0A0A0A", "#FAFAFA"

MASTER = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect x="8" y="8" width="240" height="240" rx="56" fill="{BG}" stroke="#FFFFFF" stroke-opacity="0.12" stroke-width="4"/>
<rect x="56" y="60" width="64" height="136" rx="14" fill="{FG}"/>
<rect x="143" y="67" width="50" height="122" rx="8" fill="none" stroke="{FG}" stroke-width="14"/>
</svg>'''

def hinted(n, rx, left, right, sw, rrx, border):
    # left/right = (x0, y0, x1, y1) outer pixel edges; the outline's stroke is drawn inside
    lx0, ly0, lx1, ly1 = left
    rx0, ry0, rx1, ry1 = right
    h = sw / 2
    b = (f'<rect x="0.5" y="0.5" width="{n-1}" height="{n-1}" rx="{rx-0.5}" fill="none" '
         f'stroke="#FFFFFF" stroke-opacity="0.12" stroke-width="1"/>') if border else ""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {n} {n}">
<rect x="0" y="0" width="{n}" height="{n}" rx="{rx}" fill="{BG}"/>{b}
<rect x="{lx0}" y="{ly0}" width="{lx1-lx0}" height="{ly1-ly0}" rx="{rrx}" fill="{FG}"/>
<rect x="{rx0+h}" y="{ry0+h}" width="{rx1-rx0-sw}" height="{ry1-ry0-sw}" rx="{max(rrx-h,0.25)}" fill="none" stroke="{FG}" stroke-width="{sw}"/>
</svg>'''

SMALL = {
    16: hinted(16, 3.5, (3, 3, 7, 13), (9, 3, 13, 13), 1, 1, False),
    20: hinted(20, 4.5, (4, 4, 9, 16), (11, 4, 16, 16), 1, 1.25, False),
    24: hinted(24, 5.5, (5, 5, 11, 19), (13, 5, 19, 19), 1, 1.5, True),
    32: hinted(32, 7, (7, 7, 15, 25), (17, 7, 25, 25), 2, 2, True),
}

def render(svg, n):
    png = cairosvg.svg2png(bytestring=svg.encode(), output_width=n, output_height=n)
    return Image.open(io.BytesIO(png)).convert("RGBA")

imgs = {}
for n in [16, 20, 24, 32, 40, 48, 64, 96, 128, 256, 512]:
    imgs[n] = render(SMALL.get(n, MASTER), n)
    imgs[n].save(f"{OUT}/png/twinfm-{n}.png")
open(f"{OUT}/twinfm.svg", "w").write(MASTER)

ico = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
imgs[256].save(f"{OUT}/twinfm.ico", format="ICO", sizes=[(n, n) for n in ico],
               append_images=[imgs[n] for n in ico if n != 256])
print("wrote", OUT)
