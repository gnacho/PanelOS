#!/usr/bin/env python3
"""The nixie tubes' lit digits as pictures (app/ui/timers/nixie/digit-N.png): the same strokes the tube drew as paths
(four strokes per digit: 11 / 6 / 3 / 1.2 px wide, round caps and joins, from wide orange to a white core), drawn at 4x
as discs along the flattened outline and filtered down. A picture costs the GPU one textured quad; the four wide strokes
per digit and frame were what kept the running stopwatch under 30 frames a second.
usage: python nixie_digits.py <out dir>   (digit-N.png the glow, digit-N-core.png the core)"""
import math, os, re, sys
import numpy as np
from PIL import Image, ImageDraw

DIGITS = ["M18 2 C30 2 34 16 34 36 C34 56 30 70 18 70 C6 70 2 56 2 36 C2 16 6 2 18 2 Z", "M11 13 L19 3 L19 70",
          "M4 16 C4 6 11 2 18 2 C27 2 32 8 32 17 C32 30 18 40 4 70 L33 70", "M4 4 L31 4 L16 30 C27 29 33 37 33 49 C33 62 26 70 17 70 C9 70 4 66 3 60",
          "M26 70 L26 2 L3 50 L34 50", "M31 3 L8 3 L5 32 C9 28 13 27 18 27 C28 27 33 35 33 48 C33 62 26 70 17 70 C9 70 4 66 3 60",
          "M28 6 C25 3 22 2 18 2 C8 2 3 14 3 40 C3 60 9 70 18 70 C27 70 33 62 33 52 C33 42 27 34 18 34 C10 34 4 40 3 48", "M3 3 L33 3 L13 70",
          "M18 34 C9 34 5 28 5 18 C5 8 11 2 18 2 C25 2 31 8 31 18 C31 28 27 34 18 34 C8 34 3 42 3 52 C3 63 10 70 18 70 C26 70 33 63 33 52 C33 42 28 34 18 34 Z",
          "M8 66 C11 69 14 70 18 70 C28 70 33 58 33 32 C33 12 27 2 18 2 C9 2 3 10 3 20 C3 30 9 38 18 38 C26 38 32 32 33 24"]
# (width px, colour, opacity): wide orange glow -> white core, as the tube's paths
STROKES = [(11, (0xff, 0x5a, 0x00), 0.16), (6, (0xff, 0x7a, 0x1a), 0.42), (3, (0xff, 0xb0, 0x66), 0.9), (1.2, (0xff, 0xf0, 0xdc), 1.0)]
W, H, PAD, S = 36, 72, 6, 4          # the path box, the margin for the widest stroke, the supersampling


def points(d):
    """The outline flattened to points (absolute M / L / C / Z only)."""
    toks = re.findall(r"[MLCZ]|-?\d+(?:\.\d+)?", d)
    pts, cur, start, i = [], (0.0, 0.0), (0.0, 0.0), 0
    while i < len(toks):
        t = toks[i]; i += 1
        if t == "M":
            cur = start = (float(toks[i]), float(toks[i + 1])); i += 2; pts.append([cur])
        elif t == "L":
            p = (float(toks[i]), float(toks[i + 1])); i += 2
            n = max(2, int(math.dist(cur, p) * 8))
            pts[-1] += [(cur[0] + (p[0] - cur[0]) * k / n, cur[1] + (p[1] - cur[1]) * k / n) for k in range(1, n + 1)]; cur = p
        elif t == "C":
            c1 = (float(toks[i]), float(toks[i + 1])); c2 = (float(toks[i + 2]), float(toks[i + 3])); p = (float(toks[i + 4]), float(toks[i + 5])); i += 6
            n = 120
            for k in range(1, n + 1):
                u = k / n; a = (1 - u) ** 3; b = 3 * u * (1 - u) ** 2; c = 3 * u * u * (1 - u); e = u ** 3
                pts[-1].append((a * cur[0] + b * c1[0] + c * c2[0] + e * p[0], a * cur[1] + b * c1[1] + c * c2[1] + e * p[1]))
            cur = p
        elif t == "Z":
            n = max(2, int(math.dist(cur, start) * 8))
            pts[-1] += [(cur[0] + (start[0] - cur[0]) * k / n, cur[1] + (start[1] - cur[1]) * k / n) for k in range(1, n + 1)]; cur = start
    return pts


def stroke_mask(pts, width):
    """Coverage of a round-capped, round-joined stroke: discs along the outline (4x), then a box filter."""
    m = Image.new("L", ((W + 2 * PAD) * S, (H + 2 * PAD) * S), 0)
    dr = ImageDraw.Draw(m)
    r = width / 2 * S
    for run in pts:
        for (x, y) in run:
            cx, cy = (x + PAD) * S, (y + PAD) * S
            dr.ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
    a = np.asarray(m, np.float32) / 255
    return a.reshape(H + 2 * PAD, S, W + 2 * PAD, S).mean((1, 3))


def main(out):
    os.makedirs(out, exist_ok=True)
    for k, d in enumerate(DIGITS):
        pts = points(d)
        # the glow (the three wide strokes, it flickers) and the core (the thin white stroke, steady): two pictures
        for name, strokes in ((f"digit-{k}.png", STROKES[:3]), (f"digit-{k}-core.png", STROKES[3:])):
            save(pts, strokes, os.path.join(out, name))
    print("ok", out)


def save(pts, strokes, path):
    if True:
        rgb = np.zeros((H + 2 * PAD, W + 2 * PAD, 3), np.float32)
        alpha = np.zeros((H + 2 * PAD, W + 2 * PAD), np.float32)
        for (w, c, o) in strokes:
            cov = stroke_mask(pts, w) * o
            col = np.array(c, np.float32) / 255
            # source over, premultiplied
            rgb = rgb * (1 - cov[..., None]) + col * cov[..., None]
            alpha = alpha * (1 - cov) + cov
        un = np.where(alpha[..., None] > 0, rgb / np.maximum(alpha[..., None], 1e-6), 0)
        img = np.dstack([np.clip(un, 0, 1), np.clip(alpha, 0, 1)])
        Image.fromarray((img * 255).round().astype(np.uint8), "RGBA").save(path)


if __name__ == "__main__":
    main(sys.argv[1])
