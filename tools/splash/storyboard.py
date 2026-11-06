"""Boot splash storyboard, rendered pixel-exact.

1. The logotype "L⌂UDEN" (assets/logotype.svg): white letters, red house as
   the A, on black; letters as high as the house of the round logo.
2. The letters fade, the red house glides to the logo's place (same size and
   shape as the house of assets/logo.svg).
3. The red disc grows behind it and swallows the red house (no faint lines:
   the house shows only as the white line draws).
4. The line draws itself (render_splash.Reveal: straight segments, the join
   appears with the inner roof).
5. The small logotype with the slogan fades in under the logo.
Then, until Wi-Fi is up and the panel app takes over, the small logotype dims
and a soft light band runs across it (the UI may wait for the network, the
panel is no use without it; the glow shows that it is working). Timings are
1.5x the first version, at 60 fps, so the motion stays smooth.

Writes for the panel:
  splash.bin      delta frames for nspanel-splash (format: see write_bin)
  intro.rgba      the last frame's logo + text block for the app's hand-over
  bootlogo.bmp    the first picture for the stock U-Boot (logo.bmp in the resource)
  preview.gif     the sequence (with --preview)

Usage: python storyboard.py <assets dir> <out dir> [--preview]
"""

import math
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image

import render_splash as rs
import svgraster as sr

SIZE = rs.SIZE
FPS = 60                      # the panel refreshes at ~61 Hz
SLOW = 1.5                    # 1.5x the first timing
DIM = 0.6                     # waiting: logotype brightness outside the light band
BAND = 14.0                   # light band width (Gaussian sigma, px)
FADE_S = 0.4                  # done picture <-> dimmed logotype
SWEEP_S = 1.2                 # one pass of the light band
PAUSE_S = 0.4                 # dimmed pause between passes
LOOP_STEP = 2                 # light band frames every 2 ticks (30 fps)
LOGO_CY = 214                 # logo centre: logo + text block centred on the screen
# step 1: letter height = the round logo's house height (10182 of 17632 units of its 96 px disc)
WORD_H = 10182 * rs.DISC / rs.VIEW
SMALL_H = 22                  # step 5: small logotype letter height (half of 44; slogan ~6.5 px)
TEXT_GAP = 16                 # logo bottom -> small logotype top
SLOGAN = 0.8                  # slogan brightness (letters are white)
RED, WHITE = rs.RED, rs.WHITE


def ease(t):
    return t * t * (3 - 2 * t)


def shift_y(a, dy):
    out = np.zeros_like(a)
    if dy < 0:
        out[:dy] = a[-dy:]
    elif dy > 0:
        out[dy:] = a[:-dy]
    else:
        out[:] = a
    return out


def place(subpaths, scale, tx, ty, rule):
    """Coverage of subpaths drawn at screen = p * scale + (tx, ty), full canvas."""
    x0, y0, x1, y1 = sr.bounds(subpaths)
    rx0 = max(0, int(math.floor(x0 * scale + tx)) - 2)
    ry0 = max(0, int(math.floor(y0 * scale + ty)) - 2)
    rx1 = min(SIZE, int(math.ceil(x1 * scale + tx)) + 2)
    ry1 = min(SIZE, int(math.ceil(y1 * scale + ty)) + 2)
    cov = sr.coverage(subpaths, rx1 - rx0, ry1 - ry0, scale, (rx0 - tx) / scale, (ry0 - ty) / scale, rule=rule)
    full = np.zeros((SIZE, SIZE), np.float32)
    full[ry0:ry1, rx0:rx1] = cov
    return full


def paint(img, cov, color, alpha=1.0):
    a = (cov * alpha)[..., None]
    return img * (1 - a) + color[None, None, :] * a


def build(assets):
    logo_svg = (assets / "logo.svg").read_text(encoding="utf-8")
    paths = sr.parse_paths((assets / "logotype.svg").read_text(encoding="utf-8"))
    letters = [s for c, subs in paths if c == "fil0" for s in subs]
    slogan = [s for c, subs in paths if c == "fil1" for s in subs]
    house = [s for c, subs in paths if c == "fil2" for s in subs]
    cap = 1611.0                         # letter / house height in logotype units
    total_w = 7604.13

    # Round logo at its final place (rendered centred, moved up by whole pixels).
    dy = LOGO_CY - SIZE // 2
    disc0, white0 = rs.logo_layers(logo_svg)
    reveal = rs.Reveal(white0)
    disc, white = shift_y(disc0, dy), shift_y(white0, dy)

    # Step 1: big logotype.
    sb = WORD_H / cap
    tbx, tby = SIZE / 2 - total_w * sb / 2, SIZE / 2 - WORD_H / 2
    big_letters = place(letters, sb, tbx, tby, "nonzero")
    hx0, hy0, hx1, hy1 = sr.bounds(house)
    hcx, hcy = (hx0 + hx1) / 2, (hy0 + hy1) / 2
    start_c = (tbx + hcx * sb, tby + hcy * sb)
    # The logo's house: bounding box of its white cut-out (same shape as the logotype's A).
    k = rs.DISC / rs.VIEW
    lx0, lx1, ly0, ly1 = 3691.0, 13941.0, 3030.0, 13212.0
    st = (lx1 - lx0) * k / (hx1 - hx0)
    end_c = (SIZE / 2 - rs.DISC / 2 + (lx0 + lx1) / 2 * k, LOGO_CY - rs.DISC / 2 + (ly0 + ly1) / 2 * k)

    frames = []                          # (uint8 frame, hold ticks)

    def add(img, hold=1):
        frames.append((np.clip(img + 0.5, 0, 255).astype(np.uint8), hold))

    black = np.zeros((SIZE, SIZE, 3), np.float32)
    house_big = place(house, sb, start_c[0] - hcx * sb, start_c[1] - hcy * sb, "evenodd")
    add(paint(paint(black, big_letters, WHITE), house_big, RED), hold=int(0.8 * SLOW * FPS))

    # Step 2: letters fade, house glides to the logo's place.
    n = int(0.6 * SLOW * FPS)
    for f in range(1, n + 1):
        e = ease(f / n)
        s = sb + (st - sb) * e
        cx = start_c[0] + (end_c[0] - start_c[0]) * e
        cy = start_c[1] + (end_c[1] - start_c[1]) * e
        h = place(house, s, cx - hcx * s, cy - hcy * s, "evenodd")
        fade = max(0.0, 1 - f / (0.7 * n))
        add(paint(paint(black, big_letters, WHITE, fade), h, RED))

    # Step 3: the disc grows behind the house (0.3 s); inside it the house turns into faint lines.
    yy, xx = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32) + 0.5
    dist = np.hypot(xx - SIZE / 2, yy - LOGO_CY)
    g = 0.0                              # no faint lines on the disc
    n = int(0.3 * SLOW * FPS)
    for f in range(1, n + 1):
        r = rs.DISC / 2 * (1 - (1 - f / n) ** 2)
        d = disc if f == n else np.minimum(np.clip(r - dist + 0.5, 0, 1), disc)
        on_disc = RED[None, None, :] * (1 - g * white[..., None]) + WHITE[None, None, :] * (g * white[..., None])
        off_disc = RED[None, None, :] * white[..., None]
        add(on_disc * d[..., None] + off_disc * (1 - d[..., None]))
    add(frames[-1][0].astype(np.float32), hold=int(0.2 * SLOW * FPS) - 1)

    # Step 4: draw-on.
    n = int(rs.DRAW_S * SLOW * FPS)
    for f in range(1, n + 1):
        w = g * white + (1 - g) * shift_y(reveal.shown(ease(f / n)), dy)
        face = RED[None, None, :] * (1 - w[..., None]) + WHITE[None, None, :] * w[..., None]
        add(face * disc[..., None])
    logo_img = frames[-1][0].astype(np.float32)

    # Step 5: small logotype with slogan fades in (0.6 s).
    ss_ = SMALL_H / cap
    tsx, tsy = SIZE / 2 - total_w * ss_ / 2, LOGO_CY + rs.DISC / 2 + TEXT_GAP
    s_letters = place(letters, ss_, tsx, tsy, "nonzero")
    s_slogan = place(slogan, ss_, tsx, tsy, "nonzero")
    s_house = place(house, ss_, tsx, tsy, "evenodd")
    n = int(0.6 * SLOW * FPS)
    for f in range(1, n + 1):
        a = ease(f / n)
        img = paint(logo_img, s_letters, WHITE, a)
        img = paint(img, s_slogan, WHITE * SLOGAN, a)
        img = paint(img, s_house, RED, a)
        add(img)
    done = len(frames) - 1
    # Block for the app's hand-over picture (logo + text).
    block = (int(tsx) - 2, LOGO_CY - rs.DISC // 2 - 2,
             int(math.ceil(total_w * ss_)) + 5, rs.DISC + TEXT_GAP + int(math.ceil(2604.5 * ss_)) + 5)

    # Waiting for Wi-Fi: the small logotype dims (not the slogan), a light band
    # runs across it, a dimmed pause, again. Loop entry and exit fade from and
    # to the done picture, which is also the app's first picture.
    def q(img):
        return np.clip(img + 0.5, 0, 255).astype(np.uint8)

    def word(bright):                    # bright: per column, 0..1
        bb = bright[None, :, None]
        img = paint(logo_img, s_slogan, WHITE * SLOGAN)
        a = s_letters[..., None]
        img = img * (1 - a) + WHITE[None, None, :] * bb * a
        a = s_house[..., None]
        return img * (1 - a) + RED[None, None, :] * bb * a

    d_img = frames[-1][0]
    u_img = q(word(np.full(SIZE, DIM, np.float32)))
    xs = np.arange(SIZE, dtype=np.float32) + 0.5
    x_from, x_to = tsx - 3 * BAND, tsx + total_w * ss_ + 3 * BAND
    k = int(FADE_S * FPS)
    fade_in = [(q(d_img * (1 - ease(f / k)) + u_img * ease(f / k)), 1) for f in range(1, k)] + [(u_img, 1)]
    fade_out = [(q(u_img * (1 - ease(f / k)) + d_img * ease(f / k)), 1) for f in range(1, k)] + [(d_img, 1)]
    loop = []
    steps = int(SWEEP_S * FPS / LOOP_STEP)
    for f in range(1, steps + 1):
        xc = x_from + (x_to - x_from) * f / steps
        loop.append((q(word(DIM + (1 - DIM) * np.exp(-((xs - xc) / BAND) ** 2))), LOOP_STEP))
    loop.append((u_img, int(PAUSE_S * FPS)))        # ends on the dimmed picture: loops without a seam
    return frames, done, block, (fade_in, loop, fade_out)


def delta(prev, img):
    """Spans of changed pixels: u16 y, u16 x, u16 n, n x RGB."""
    changed = np.any(img != prev, axis=2)
    body = []
    for y in np.nonzero(changed.any(axis=1))[0]:
        xs = np.nonzero(changed[y])[0]
        # merge runs closer than 4 pixels (span header = 6 bytes)
        start = last = xs[0]
        for x in list(xs[1:]) + [None]:
            if x is not None and x - last <= 4:
                last = x
                continue
            n = last - start + 1
            body.append(struct.pack("<HHH", int(y), int(start), int(n)) + img[y, start:last + 1].tobytes())
            if x is not None:
                start = last = x
    return b"".join(body)


def write_bin(frames, done, waiting, path):
    """splash.bin v3: "NSPA", u16 version=3, u16 fps, u16 sequence records,
    u16 done record, u16 fade-in records, u16 loop records, u16 fade-out
    records; then the records in that order, each u16 hold (ticks of 1/fps),
    u32 bytes, spans against the previous picture:
      sequence  black -> ... -> D (done picture)
      fade-in   D -> ... -> U (dimmed logotype)
      loop      U -> light band frames -> U (so it repeats without a seam)
      fade-out  U -> ... -> D
    """
    fade_in, loop, fade_out = waiting
    out = [b"NSPA", struct.pack("<HHHHHHH", 3, FPS, len(frames), done, len(fade_in), len(loop), len(fade_out))]
    prev = np.zeros((SIZE, SIZE, 3), np.uint8)
    for part in (frames, fade_in, loop, fade_out):
        for img, hold in part:
            data = delta(prev, img)
            out.append(struct.pack("<HI", hold, len(data)) + data)
            prev = img
    assert np.array_equal(fade_in[-1][0], loop[-1][0]) and np.array_equal(fade_out[-1][0], frames[-1][0])
    Path(path).write_bytes(b"".join(out))


def main():
    assets, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    frames, done, (bx, by, bw, bh), waiting = build(assets)
    write_bin(frames, done, waiting, out / "splash.bin")
    # Bootloader logo: the first picture, 480x480 8-bit palette BMP like the stock logo.bmp.
    logo = Image.fromarray(frames[0][0]).quantize(256, dither=Image.Dither.NONE)
    pal = logo.getpalette()[:768]
    logo.putpalette(pal + [0] * (768 - len(pal)))     # always 256 entries, as in the stock file
    logo.save(out / "bootlogo.bmp")
    last = frames[-1][0]
    crop = last[by:by + bh, bx:bx + bw]
    rgba = np.dstack([crop, np.full(crop.shape[:2], 255, np.uint8)])
    (out / "intro.rgba").write_bytes(struct.pack("<HHHH", bx, by, bw, bh) + rgba.tobytes())
    if "--preview" in sys.argv:
        imgs, durs = [], []
        for img, hold in frames:
            imgs.append(Image.fromarray(img))
            durs.append(int(1000 * hold / FPS))
        imgs.append(imgs[-1])
        durs.append(1000)
        fade_in, loop, fade_out = waiting
        for img, hold in fade_in + loop + loop + fade_out:
            imgs.append(Image.fromarray(img))
            durs.append(int(1000 * hold / FPS))
        imgs.append(imgs[-1])
        durs.append(1500)
        # GIF delays are in 10 ms steps: merge 60 fps frames in pairs for the preview
        imgs, durs = imgs[::2], [a + b for a, b in zip(durs[::2], durs[1::2] + [0])]
        imgs[0].save(out / "preview.gif", save_all=True, append_images=imgs[1:], duration=durs, loop=0)
        n = len(frames)
        keys = [0, n * 3 // 10, n * 45 // 100, n // 2, n * 6 // 10, n * 75 // 100, n - 1, n + 40, n + 55]
        imgs = [Image.fromarray(img) for img, _ in frames + fade_in + loop]
        strip = Image.new("RGB", (SIZE * len(keys) + 6 * (len(keys) - 1), SIZE), (60, 60, 60))
        for i, k in enumerate(keys):
            strip.paste(imgs[min(k, len(imgs) - 1)], (i * (SIZE + 6), 0))
        strip.save(out / "keyframes.png")
    size = (out / "splash.bin").stat().st_size
    fade_in, loop, _ = waiting
    print(f"{len(frames)} frames ({sum(h for _, h in frames) / FPS:.2f} s), done at {done}, "
          f"loop {len(loop)} records ({sum(h for _, h in loop) / FPS:.2f} s), "
          f"splash.bin {size} bytes, block {bw}x{bh} at {bx},{by}")


if __name__ == "__main__":
    main()
