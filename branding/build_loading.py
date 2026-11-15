"""Background of the panel's waiting screens (the dashboard's loading screen,
connecting, Home Assistant not reachable): the Morse pattern, calmed down as in
the start-up film, with a rounded calm panel behind the mark (2 mark units of
clear space). Dark: the dark pattern on the panel's black; light: the light
pattern on Paper. Writes app/ui/images/loading.png (dark, mark in the middle,
under the web view's loading line) and wait-dark.png / wait-light.png (mark
higher than in the start-up film, room for the texts and buttons of the
connecting and offline screens: centred at y 180), all 480 x 480; the app draws the mark on top
(app/ui/loading.slint MorseWrite, mark unit 16 px).

Run: python branding/build_loading.py
"""

from pathlib import Path

from PIL import Image, ImageDraw

import build_brand as bb

SIZE = 480
SS = 4                 # supersampling
PATTERN_U = 6          # pattern unit on the panel, px
MARK_U = 16            # mark unit in loading.slint, px
CALM = 0.6             # pattern brightness, as in the film (calm, not loud)
IMAGES = Path(__file__).resolve().parent.parent / "app" / "ui" / "images"


def render(dark, cy, name):
    ground = (0, 0, 0) if dark else bb.hex_rgb(bb.PAPER)
    img = Image.new("RGB", (SIZE * SS, SIZE * SS), ground)
    d = ImageDraw.Draw(img)
    cols = bb.PATTERN_DARK if dark else bb.PATTERN_LIGHT
    # calm: the pattern colour moved towards the ground
    dim = lambda h: tuple(int(g + (c - g) * CALM) for c, g in zip(bb.hex_rgb(h), ground))
    dot, dash = dim(cols["dot"]), dim(cols["dash"])
    els, w = bb.signature_elements(PATTERN_U)
    period = w + bb.REPEAT_GAP * PATTERN_U
    y, k = -PATTERN_U, 0
    while y < SIZE + PATTERN_U:
        x0 = -((k * bb.ROW_SHIFT * PATTERN_U) % period)
        while x0 < SIZE:
            for kind, x, ey, ew, eh in els:
                box = [(x0 + x) * SS, (y + ey) * SS, (x0 + x + ew) * SS, (y + ey + eh) * SS]
                if kind == "dot":
                    d.ellipse(box, fill=dot)
                else:
                    d.rounded_rectangle(box, radius=eh * SS / 2, fill=dash)
            x0 += period
        y += bb.ROW_PITCH * PATTERN_U
        k += 1
    # the calm panel behind the mark (5 x 3 units + 2 units around)
    mw, mh, pad = 5 * MARK_U, 3 * MARK_U, 2 * MARK_U
    x0, y0 = (SIZE - mw) / 2 - pad, cy - mh / 2 - pad
    d.rounded_rectangle([x0 * SS, y0 * SS, (x0 + mw + 2 * pad) * SS, (y0 + mh + 2 * pad) * SS],
                        radius=MARK_U * 0.9 * SS, fill=ground)
    out = IMAGES / name
    out.parent.mkdir(parents=True, exist_ok=True)
    img.resize((SIZE, SIZE), Image.LANCZOS).save(out, optimize=True)
    print(out, out.stat().st_size)


def main():
    render(True, SIZE / 2, "loading.png")
    render(True, 180, "wait-dark.png")
    render(False, 180, "wait-light.png")


if __name__ == "__main__":
    main()
