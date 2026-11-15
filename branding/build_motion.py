"""PanelOS start-up, as a 16:9 film for presentations (brand kit, motion/).

Black, the quiet pattern fades in, the mark writes itself in Morse with its
tone, the name appears under it, a pause, back to black. The logo is small and
centred on a calm black panel: the stacked logo's proportions, the mark 6.5 % of
the frame's width.
Writes kit/motion/panelos-start-up-3840x2160.mp4 and -1920x1080.mp4 (60 fps,
with the tone) and a contact sheet next to this script.

Run after build_brand.py: python branding/build_motion.py   (needs ffmpeg, Edge)
"""

from __future__ import annotations

import math
import shutil
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

import build_brand as bb

W, H, FPS = 3840, 2160, 60
OUT = bb.KIT / "motion"
FADE_IN, LEAD = 0.8, 1.0            # black to pattern; first tone at LEAD
HOLD, FADE_OUT = 2.2, 0.8

MARK_W = 0.065 * W                  # mark width
U = MARK_W / 5                      # mark unit
CAP = 2.5 * U                       # stacked logo: wordmark cap is 2.5 mark units
GAP = 3.2 * U                       # between the mark and the cap line
BLOCK_H = 3 * U + GAP + CAP
MX, MY = (W - 5 * U) / 2, (H - BLOCK_H) / 2
PATTERN_U = 12                      # well under a quarter of the mark's unit

SIGNAL = np.array(bb.hex_rgb(bb.SIGNAL), np.float32)
WHITE = np.array((255, 255, 255), np.float32)

# timing: P in Morse, unit 100 ms
EVENTS, t = [], LEAD
for ch in ".--.":
    dur = bb.TONE_UNIT if ch == "." else 3 * bb.TONE_UNIT
    EVENTS.append((ch, t, dur))
    t += dur + bb.TONE_UNIT
T_CODE_END = t - bb.TONE_UNIT
T_WORD = T_CODE_END + 0.15
T_FADE = T_WORD + 0.45 + HOLD
T_END = T_FADE + FADE_OUT


def ease_out(x):
    x = min(max(x, 0.0), 1.0)
    return 1 - (1 - x) ** 3


def smooth(x):
    x = min(max(x, 0.0), 1.0)
    return x * x * (3 - 2 * x)


def pattern_background() -> np.ndarray:
    ss = 2
    img = Image.new("RGB", (W * ss, H * ss), (0, 0, 0))
    d = ImageDraw.Draw(img)
    els, w = bb.signature_elements(PATTERN_U)
    period = w + bb.REPEAT_GAP * PATTERN_U
    y, k = 0.0, 1
    while y < H + PATTERN_U:
        x0 = -((k * bb.ROW_SHIFT * PATTERN_U) % period)
        while x0 < W:
            for kind, x, ey, ew, eh in els:
                box = [(x0 + x) * ss, (y + ey) * ss, (x0 + x + ew) * ss, (y + ey + eh) * ss]
                if kind == "dot":
                    d.ellipse(box, fill=bb.hex_rgb(bb.PATTERN_DARK["dot"]))
                else:
                    d.rounded_rectangle(box, radius=eh * ss / 2, fill=bb.hex_rgb(bb.PATTERN_DARK["dash"]))
            x0 += period
        y += bb.ROW_PITCH * PATTERN_U
        k += 1
    # a calm panel behind the logo, as on the covers of the guidelines: the
    # ground's colour with round corners, 2u of clear space around the logo
    pad = 2 * U
    word_w = CAP * 5.422
    x0, x1 = W / 2 - max(5 * U, word_w) / 2 - pad, W / 2 + max(5 * U, word_w) / 2 + pad
    y0, y1 = MY - pad, MY + BLOCK_H + pad * 0.9
    d.rounded_rectangle([x0 * ss, y0 * ss, x1 * ss, y1 * ss], radius=U * 0.9 * ss, fill=(0, 0, 0))
    return np.asarray(img.resize((W, H), Image.LANCZOS), dtype=np.float32)


def word_layer():
    geo = bb.wordmark_geometry()
    k = CAP / geo["cap_px"]
    mask = Image.fromarray(geo["mask"]).resize((round(geo["mask"].shape[1] * k), round(geo["mask"].shape[0] * k)), Image.LANCZOS)
    alpha = np.asarray(mask, dtype=np.float32)[..., None] / 255
    base_y = MY + 3 * U + GAP + CAP          # baseline
    top = base_y - geo["base_px"] * k
    left = (W - alpha.shape[1]) / 2
    return alpha, int(round(left)), int(round(top))


# region that holds the mark, its glow and the name
RX0, RY0 = int(MX - 3 * U), int(MY - 3 * U)
RX1, RY1 = int(MX + 8 * U), int(MY + BLOCK_H + 2 * U)
SS = 3


def mark_region(t):
    w, h = RX1 - RX0, RY1 - RY0
    layer = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
    glow = Image.new("RGB", (w, h), (0, 0, 0))
    d, g = ImageDraw.Draw(layer), ImageDraw.Draw(glow)
    for (ch, start, dur), (kind, x, y, ew, eh) in zip(EVENTS, bb.mark_elements(U, MX - RX0, MY - RY0)):
        p = (t - start) / 0.16
        if p <= 0:
            continue
        if kind == "dot":
            s = ease_out(p) * (1 + 0.12 * math.sin(min(p, 1) * math.pi))
            r = U / 2 * s
            cx, cy = x + ew / 2, y + eh / 2
            d.ellipse([(cx - r) * SS, (cy - r) * SS, (cx + r) * SS, (cy + r) * SS], fill=bb.hex_rgb(bb.SIGNAL) + (255,))
            gl = max(0.0, 1 - (t - start) / 0.9)
            if gl > 0:
                rr = U * 0.95
                g.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], fill=tuple(int(c * 0.55 * gl) for c in bb.hex_rgb(bb.SIGNAL)))
        else:
            grow = ease_out((t - start) / dur)
            wd = max(eh, ew * grow)
            d.rounded_rectangle([x * SS, y * SS, (x + wd) * SS, (y + eh) * SS], radius=eh * SS / 2, fill=(255, 255, 255, 255))
            gl = max(0.0, 1 - (t - start) / (dur + 0.5))
            if gl > 0:
                pad = U * 0.18
                g.rounded_rectangle([x - pad, y - pad, x + wd + pad, y + eh + pad], radius=eh / 2 + pad, fill=tuple(int(70 * gl) for _ in range(3)))
    layer = np.asarray(layer.resize((w, h), Image.LANCZOS), dtype=np.float32)
    halo = np.asarray(glow.filter(ImageFilter.GaussianBlur(U * 0.35)), dtype=np.float32)
    return layer, halo


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    bg = pattern_background()
    walpha, wx, wy = word_layer()
    tmp = Path(tempfile.mkdtemp())
    wav = tmp / "tone.wav"
    bb.tone_wav(wav, lead=LEAD, tail=T_END - T_CODE_END)
    ffmpeg = shutil.which("ffmpeg") or str(next(Path.home().glob(
        "AppData/Local/Microsoft/WinGet/Packages/Gyan.FFmpeg*/ffmpeg-*/bin/ffmpeg.exe")))
    f4k, fhd = OUT / "panelos-start-up-3840x2160.mp4", OUT / "panelos-start-up-1920x1080.mp4"
    proc = subprocess.Popen(
        [ffmpeg, "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
         "-i", str(wav), "-filter_complex", "[0:v]split=2[a][b];[b]scale=1920:1080:flags=lanczos[hd]",
         "-map", "[a]", "-map", "1:a", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "14", "-preset", "slow",
         "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart", str(f4k),
         "-map", "[hd]", "-map", "1:a", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16", "-preset", "slow",
         "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart", str(fhd)], stdin=subprocess.PIPE)
    n = int(round(T_END * FPS))
    sheet_t = [0.4, EVENTS[0][1] + 0.08, EVENTS[1][1] + 0.2, EVENTS[3][1] + 0.08, T_WORD + 0.6, T_FADE + 0.4]
    sheet = []
    for i in range(n):
        t = i / FPS
        frame = bg * (0.45 * smooth(t / FADE_IN))
        layer, halo = mark_region(t)
        reg = frame[RY0:RY1, RX0:RX1]
        reg += halo
        a = layer[..., 3:4] / 255
        reg[:] = reg * (1 - a) + layer[..., :3] * a
        wa = ease_out((t - T_WORD) / 0.45)
        if wa > 0:
            dy = int(round(U * 0.25 * (1 - wa)))
            h, w = walpha.shape[:2]
            sl = frame[wy + dy:wy + dy + h, wx:wx + w]
            al = walpha * wa
            sl[:] = sl * (1 - al) + 255 * al
        if t > T_FADE:
            frame *= max(0.0, 1 - (t - T_FADE) / FADE_OUT)
        out = np.clip(frame, 0, 255).astype(np.uint8)
        proc.stdin.write(out.tobytes())
        if any(abs(t - s) < 0.5 / FPS for s in sheet_t):
            sheet.append(Image.fromarray(out).resize((480, 270), Image.LANCZOS))
    proc.stdin.close()
    proc.wait()
    cs = Image.new("RGB", (480 * 3, 270 * 2))
    for k, im in enumerate(sheet[:6]):
        cs.paste(im, ((k % 3) * 480, (k // 3) * 270))
    cs.save(Path(__file__).parent / "motion-sheet.png")
    shutil.rmtree(tmp, ignore_errors=True)
    print(f"{n} frames, {T_END:.2f} s ->", f4k.stat().st_size, fhd.stat().st_size)


if __name__ == "__main__":
    main()
