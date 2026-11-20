"""Draws DreamsCraft's app icon — layered luminous animation motion arcs and a glowing play-spark
emblem in a cyan-violet-magenta-gold dream gradient on deep obsidian — and writes every size
the Android app, Desktop app, and repository use. Needs Pillow. Run from anywhere:

    python3 tools/app-icon/make_icon.py

Writes:
- assets/app-icon/procraft-1024.png, procraft-512.png, procraft-256.png (and dreamscraft-* aliases)
- assets/app-icon/procraft.ico (and dreamscraft.ico)
- assets/app-icon/hicolor/<n>x<n>/apps/dreamscraft.png (and procraft.png)
- Android adaptive icon layers (`drawable-*dpi/ic_launcher_foreground.png`,
  `ic_launcher_monochrome.png`) and the About screen mark (`drawable-nodpi/procraft_mark.png`).
"""

import math
import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RES = os.path.join(ROOT, 'android', 'app', 'src', 'main', 'res')

S = 2048  # 512-unit tile rendered at 4x for razor-sharp antialiasing
BG_INK = (12, 10, 22)
# DreamsCraft signature gradient: electric cyan -> indigo-violet -> hot magenta -> warm amber gold
STOPS = [
    (0.00, (14, 218, 245)),
    (0.32, (118, 86, 255)),
    (0.66, (245, 58, 134)),
    (1.00, (255, 192, 56)),
]


def blend(t):
    t = min(1.0, max(0.0, t))
    for (a, ca), (b, cb) in zip(STOPS, STOPS[1:]):
        if t <= b:
            u = (t - a) / max(b - a, 1e-6)
            return tuple(round(ca[i] + (cb[i] - ca[i]) * u) for i in range(3))
    return STOPS[-1][1]


def diagonal_gradient():
    """Smooth diagonal dream gradient across S x S."""
    w = 512
    img = Image.new('RGB', (w, w))
    px = img.load()
    for y in range(w):
        for x in range(w):
            t = (0.62 * x + 0.38 * y) / w
            px[x, y] = blend(t)
    return img.resize((S, S), Image.BICUBIC)


def rounded_play_mask(cx, cy, radius, corner_r):
    """Draws a smooth, rounded right-pointing animation play triangle centered at (cx, cy)."""
    pts = []
    for deg in (0, 120, 240):
        rad = math.radians(deg)
        pts.append((cx + radius * math.cos(rad), cy + radius * math.sin(rad)))

    # Inset vertices so corner circles stay cleanly tangent
    mask = Image.new('L', (S, S), 0)
    d = ImageDraw.Draw(mask)
    inset_pts = []
    for x, y in pts:
        vx, vy = x - cx, y - cy
        length = math.hypot(vx, vy)
        scale = (length - corner_r * 1.15) / max(length, 1e-6)
        inset_pts.append((cx + vx * scale, cy + vy * scale))
    # Build smooth rounded polygon via thick stroke + fill + corner circles
    d.polygon(inset_pts, fill=255)
    for i in range(3):
        p0 = inset_pts[i]
        p1 = inset_pts[(i + 1) % 3]
        d.line([p0, p1], fill=255, width=round(corner_r * 2))
    for px, py in inset_pts:
        d.ellipse((px - corner_r, py - corner_r, px + corner_r, py + corner_r), fill=255)
    return mask


def crescent_arc_mask(cx, cy, r_outer, r_inner, start_deg, end_deg, shift_x=0.0):
    """Smooth onion-skin motion trail arc behind the play emblem."""
    mask = Image.new('L', (S, S), 0)
    d = ImageDraw.Draw(mask)
    steps = 96
    outer = []
    inner = []
    for i in range(steps + 1):
        t = i / steps
        ang = math.radians(start_deg + (end_deg - start_deg) * t)
        # Taper thickness softly at the tips
        taper = math.sin(t * math.pi) ** 0.55
        thick = (r_outer - r_inner) * taper
        rmid = 0.5 * (r_outer + r_inner)
        ro = rmid + thick * 0.5
        ri = rmid - thick * 0.5
        outer.append((cx + shift_x + ro * math.cos(ang), cy + ro * math.sin(ang)))
        inner.append((cx + shift_x + ri * math.cos(ang), cy + ri * math.sin(ang)))
    poly = outer + list(reversed(inner))
    d.polygon(poly, fill=255)
    return mask


def emblem():
    """DreamsCraft emblem: outer film-ring motion arcs + onion-skin echo + luminous play triangle."""
    grad = diagonal_gradient()
    layer = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    mono_mask = Image.new('L', (S, S), 0)

    cx, cy = S * 0.50, S * 0.50

    # 1. Sweeping outer cosmic-motion ring (two dynamic arcs framing the animation stage)
    arc1 = crescent_arc_mask(cx, cy, r_outer=780, r_inner=640, start_deg=142, end_deg=332)
    arc2 = crescent_arc_mask(cx, cy, r_outer=780, r_inner=655, start_deg=-14, end_deg=116)
    ring_mask = ImageChops.lighter(arc1, arc2)

    ring_rgba = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    ring_rgba.paste(grad, (0, 0), ring_mask)
    layer.alpha_composite(ring_rgba)
    mono_mask = ImageChops.lighter(mono_mask, ring_mask)

    # 2. Onion-skin ghost echoes trailing behind the play emblem (signature animation motif!)
    echo2 = rounded_play_mask(cx - 175, cy, radius=325, corner_r=64)
    echo2_soft = echo2.point(lambda a: round(a * 0.28))
    echo2_rgba = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    echo2_rgba.paste(grad, (0, 0), echo2_soft)
    layer.alpha_composite(echo2_rgba)

    echo1 = rounded_play_mask(cx - 82, cy, radius=365, corner_r=70)
    echo1_soft = echo1.point(lambda a: round(a * 0.55))
    echo1_rgba = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    echo1_rgba.paste(grad, (0, 0), echo1_soft)
    layer.alpha_composite(echo1_rgba)

    # 3. Primary luminous rounded Play emblem in the foreground
    play_m = rounded_play_mask(cx + 55, cy, radius=410, corner_r=78)
    play_rgba = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    play_rgba.paste(grad, (0, 0), play_m)

    # Specular inner sheen on top half of the play glyph for jewel-like depth
    sheen = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    sd = ImageDraw.Draw(sheen)
    sd.ellipse((cx - 260, cy - 420, cx + 440, cy + 70), fill=(255, 255, 255, 68))
    sheen_a = ImageChops.multiply(sheen.split()[3], play_m)
    sheen.putalpha(sheen_a)
    play_rgba.alpha_composite(sheen)

    # Subtle bright core spark dot on the right tip of the play arrow
    spark = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    spd = ImageDraw.Draw(spark)
    spd.ellipse((cx + 390, cy - 68, cx + 526, cy + 68), fill=(255, 248, 224, 150))
    spark = spark.filter(ImageFilter.GaussianBlur(28))
    spark.putalpha(ImageChops.multiply(spark.split()[3], play_m))
    play_rgba.alpha_composite(spark)

    layer.alpha_composite(play_rgba)
    mono_mask = ImageChops.lighter(mono_mask, play_m)

    # Soft luminous aura behind everything
    aura_alpha = layer.split()[3].filter(ImageFilter.GaussianBlur(90)).point(lambda a: min(255, round(a * 0.48)))
    aura = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    aura.paste(grad, (0, 0), aura_alpha)
    out = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    out.alpha_composite(aura)
    out.alpha_composite(layer)
    return out, mono_mask


def tile_mask():
    m = Image.new('L', (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, S - 1, S - 1), radius=112 * 4, fill=255)
    return m


def adaptive(picture):
    """`picture` (the tile's contents) placed in the middle 72 of the adaptive icon's 108."""
    side = round(S * 108 / 72)
    out = Image.new('RGBA', (side, side), (0, 0, 0, 0))
    out.alpha_composite(picture, ((side - S) // 2, (side - S) // 2))
    return out


def save(image, path, size):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    image.resize((size, size), Image.LANCZOS).save(path, optimize=True)


def main():
    picture, mono_mask = emblem()

    # Deep obsidian-indigo tile with a subtle radial spotlight
    tile = Image.new('RGBA', (S, S), BG_INK + (255,))
    spot = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(spot).ellipse((240, 240, S - 240, S - 240), fill=(52, 34, 96, 115))
    spot = spot.filter(ImageFilter.GaussianBlur(260))
    tile.alpha_composite(spot)
    tile.alpha_composite(picture)
    tile.putalpha(tile_mask())

    for size in (1024, 512, 256):
        save(tile, os.path.join(ROOT, 'assets', 'app-icon', f'procraft-{size}.png'), size)
        save(tile, os.path.join(ROOT, 'assets', 'app-icon', f'dreamscraft-{size}.png'), size)
    sizes = [(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)]
    tile.resize((256, 256), Image.LANCZOS).save(os.path.join(ROOT, 'assets', 'app-icon', 'procraft.ico'), sizes=sizes)
    tile.resize((256, 256), Image.LANCZOS).save(os.path.join(ROOT, 'assets', 'app-icon', 'dreamscraft.ico'), sizes=sizes)
    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        save(tile, os.path.join(ROOT, 'assets', 'app-icon', 'hicolor', f'{size}x{size}', 'apps', 'dreamscraft.png'), size)
        save(tile, os.path.join(ROOT, 'assets', 'app-icon', 'hicolor', f'{size}x{size}', 'apps', 'procraft.png'), size)

    foreground = adaptive(picture)
    mono = Image.new('RGBA', (S, S), (255, 255, 255, 0))
    mono.putalpha(mono_mask)
    monochrome = adaptive(mono)
    for bucket, scale in (('mdpi', 1), ('hdpi', 1.5), ('xhdpi', 2), ('xxhdpi', 3), ('xxxhdpi', 4)):
        size = round(108 * scale)
        save(foreground, os.path.join(RES, f'drawable-{bucket}', 'ic_launcher_foreground.png'), size)
        save(monochrome, os.path.join(RES, f'drawable-{bucket}', 'ic_launcher_monochrome.png'), size)
    # About screen mark (450 dp @ 3x)
    save(foreground, os.path.join(RES, 'drawable-nodpi', 'procraft_mark.png'), 1350)
    print('DreamsCraft icons written successfully!')


if __name__ == '__main__':
    main()
