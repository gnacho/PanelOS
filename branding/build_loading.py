"""Background of the panel's loading screen (dashboard mode): the dark Morse
pattern on the panel's black, calmed down as in the start-up film, with a
rounded black panel behind the mark (2 mark units of clear space).
Writes app/ui/images/loading.png (480 x 480); the app draws the mark on top
(app/ui/loading.slint, mark unit 16 px).

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
OUT = Path(__file__).resolve().parent.parent / "app" / "ui" / "images" / "loading.png"


def main():
    img = Image.new("RGB", (SIZE * SS, SIZE * SS), (0, 0, 0))
    d = ImageDraw.Draw(img)
    dim = lambda h: tuple(int(c * CALM) for c in bb.hex_rgb(h))
    dot, dash = dim(bb.PATTERN_DARK["dot"]), dim(bb.PATTERN_DARK["dash"])
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
    x0, y0 = (SIZE - mw) / 2 - pad, (SIZE - mh) / 2 - pad
    d.rounded_rectangle([x0 * SS, y0 * SS, (x0 + mw + 2 * pad) * SS, (y0 + mh + 2 * pad) * SS],
                        radius=MARK_U * 0.9 * SS, fill=(0, 0, 0))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    img.resize((SIZE, SIZE), Image.LANCZOS).save(OUT, optimize=True)
    print(OUT, OUT.stat().st_size)


if __name__ == "__main__":
    main()
