"""PanelOS brand: the mark's geometry, the wordmark, and every exported file.

The mark is the letter P in Morse code, dot dash dash dot, set in two rows:
dot and dash over dash and dot. Everything is measured in one unit u: a dot
is u wide, a dash 3u long and u high, the space between two elements u, the
space between the rows u. The mark is 5u by 3u and reads the same after a
half turn, because .--. is the same from either end.

The wordmark is Space Grotesk Medium (SIL Open Font License). A headless Edge
renders it from Google Fonts on a transparent page; the alpha channel gives
the PNGs their exact shapes, and its traced outline (OpenCV) gives the SVGs.
The script stops if the font did not load.

Run: python branding/build_brand.py   (needs Pillow, numpy, OpenCV, Edge)
"""

from __future__ import annotations

import io
import json
import math
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
HA_BRAND = REPO / "custom_components" / "panelos" / "brand"
OUT = HERE  # vector sources and previews
KIT = HERE / "kit"

# ---------- colours ----------
SIGNAL = "#FF4A1C"   # the dots: the lit part of the code
INK = "#101114"      # dashes and text on light grounds
PAPER = "#F5F3EE"    # light ground; dashes and text on dark grounds
NIGHT = "#000000"    # the panel's own screen
GRAPHITE = "#121314" # dark ground in documents and on the web
# the pattern: the code as texture, low contrast
PATTERN_DARK = {"dot": "#4A1A12", "dash": "#1E1F23"}
PATTERN_LIGHT = {"dot": "#F6C7B8", "dash": "#E4E1D9"}

WORD = "PanelOS"
FONT = "Space Grotesk"
WEIGHT = 500
# Logo: the wordmark's cap height is 4u, so the mark (3u high) is three
# quarters of it; the space between the mark and the P is 2u (chosen after
# comparing 1.0, 0.83, 0.75, 0.71 and 0.62). Stacked: the mark is centred above.
CAP_UNITS = 4.0
GAP_UNITS = 2.0
STACK_MARK_SCALE = 1.6   # stacked logo: the mark's unit over the wordmark's


def hex_rgb(h: str) -> tuple[int, int, int]:
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


# ---------- the mark ----------
def mark_elements(u: float = 1.0, x0: float = 0.0, y0: float = 0.0):
    """(kind, x, y, w, h) of the four elements, top-left based."""
    return [
        ("dot", x0, y0, u, u),
        ("dash", x0 + 2 * u, y0, 3 * u, u),
        ("dash", x0, y0 + 2 * u, 3 * u, u),
        ("dot", x0 + 4 * u, y0 + 2 * u, u, u),
    ]


def svg_elements(u: float, x0: float, y0: float, dot: str, dash: str) -> str:
    out = []
    for kind, x, y, w, h in mark_elements(u, x0, y0):
        if kind == "dot":
            out.append(f'<circle cx="{x + w / 2:.3f}" cy="{y + h / 2:.3f}" r="{w / 2:.3f}" fill="{dot}"/>')
        else:
            out.append(f'<rect x="{x:.3f}" y="{y:.3f}" width="{w:.3f}" height="{h:.3f}" rx="{h / 2:.3f}" fill="{dash}"/>')
    return "".join(out)


def svg_doc(inner: str, w: float, h: float, label: str) -> str:
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.2f} {h:.2f}" width="{w:.2f}" height="{h:.2f}" '
            f'role="img" aria-label="{label}"><title>{label}</title>{inner}</svg>\n')


MARK_COLOURS = {
    "color": (SIGNAL, INK),
    "color-dark": (SIGNAL, PAPER),
    "mono-ink": (INK, INK),
    "mono-white": ("#FFFFFF", "#FFFFFF"),
}


def svg_mark(name: str, u: float = 20.0) -> str:
    dot, dash = MARK_COLOURS[name]
    return svg_doc(svg_elements(u, 0, 0, dot, dash), 5 * u, 3 * u, "PanelOS")


# ---------- the app icon (badge) ----------
ICON_MARK_WIDTH = 0.62   # the mark's width over the badge's
SQUIRCLE_N = 4.0         # superellipse exponent: continuous curvature, every side gently curved
ICON_INSET = 0.985       # the badge's half-width over the canvas' (antialiasing never clips an edge)


def squircle_path(cx: float, cy: float, r: float, n: float = SQUIRCLE_N, steps: int = 160) -> str:
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        pts.append((cx + r * math.copysign(abs(c) ** (2 / n), c), cy + r * math.copysign(abs(s) ** (2 / n), s)))
    return "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + " Z"


def icon_inner(size: float = 512.0, ground: str = INK, dot: str = SIGNAL, dash: str = PAPER) -> str:
    u = size * ICON_MARK_WIDTH / 5
    x0, y0 = (size - 5 * u) / 2, (size - 3 * u) / 2
    return f'<path d="{squircle_path(size / 2, size / 2, size / 2 * ICON_INSET)}" fill="{ground}"/>' + svg_elements(u, x0, y0, dot, dash)


# ---------- the signature: P O S in Morse ----------
LETTERS = {"P": ".--.", "O": "---", "S": "..."}


def signature_elements(u: float = 1.0, x0: float = 0.0, y0: float = 0.0, word: str = "POS"):
    """Elements of a word in Morse on one line: u between elements, 3u between letters."""
    out, x = [], x0
    for li, letter in enumerate(word):
        for ch in LETTERS[letter]:
            if ch == ".":
                out.append(("dot", x, y0, u, u)); x += u
            else:
                out.append(("dash", x, y0, 3 * u, u)); x += 3 * u
            x += u
        x += 2 * u  # letter space: 3u in all
    return out, x - 3 * u - x0


def svg_signature(dot: str, dash: str, u: float = 10.0) -> tuple[str, float]:
    els, w = signature_elements(u)
    inner = []
    for kind, x, y, ew, eh in els:
        if kind == "dot":
            inner.append(f'<circle cx="{x + ew / 2:.3f}" cy="{y + eh / 2:.3f}" r="{ew / 2:.3f}" fill="{dot}"/>')
        else:
            inner.append(f'<rect x="{x:.3f}" y="{y:.3f}" width="{ew:.3f}" height="{eh:.3f}" rx="{eh / 2:.3f}" fill="{dash}"/>')
    return "".join(inner), w


# ---------- the pattern ----------
ROW_PITCH = 2.6   # rows, in units
ROW_SHIFT = 7     # each row moves 7u to the left
REPEAT_GAP = 3    # space between two signatures in a row, in units


def pattern_inner(width: float, height: float, u: float, dot: str, dash: str, phase: int = 0, sid: str = "sig") -> str:
    """Rows of P O S. `sid` names the shared row element: it must be unique on a page."""
    sig, w = svg_signature(dot, dash, u)
    period = w + REPEAT_GAP * u
    rows, y, k = [], 0.0, phase
    while y < height + u:
        x = -((k * ROW_SHIFT * u) % period)
        row = []
        while x < width:
            row.append(f'<use href="#{sid}" x="{x:.2f}"/>')
            x += period
        rows.append(f'<g transform="translate(0 {y:.2f})">{"".join(row)}</g>')
        y += ROW_PITCH * u
        k += 1
    return f'<defs><g id="{sid}">{sig}</g></defs>' + "".join(rows)


# ---------- banners: README header and social preview ----------
def banner_svg(geo, w: int, h: int, dark: bool = True, sub: str = "Open firmware for smart home wall panels",
               logo_frac: float = 0.30, sid: str = "bsig") -> str:
    """Ground with the pattern, the logo and one line in a calm panel."""
    ground = GRAPHITE if dark else PAPER
    cols = PATTERN_DARK if dark else PATTERN_LIGHT
    fg = PAPER if dark else INK
    s = f'<rect width="{w}" height="{h}" fill="{ground}"/>' + pattern_inner(w, h, h / 26, cols["dot"], cols["dash"], 2, sid)
    L = logo_layout(geo, False, 20.0)
    k = (h * logo_frac) / L["h"]
    lw = L["w"] * k
    sub_px = h * logo_frac * 0.21
    sub_w = len(sub) * sub_px * 0.52
    pad = h * logo_frac * 0.40
    bw, bh = max(lw, sub_w) + 2 * pad, L["h"] * k + sub_px * 2.6 + 2 * pad * 0.8
    bx, by = w * 0.06, (h - bh) / 2
    s += f'<rect x="{bx:.1f}" y="{by:.1f}" width="{bw:.1f}" height="{bh:.1f}" rx="{h * logo_frac * 0.16:.1f}" fill="{ground}"/>'
    mx, my, mu = L["mark"]
    wx, wy, cap = L["word"]
    ox, oy = bx + pad, by + pad * 0.8
    s += f'<g transform="translate({ox:.2f} {oy:.2f}) scale({k:.5f})">' + svg_elements(mu, mx, my, SIGNAL, fg)
    s += f'<path d="{geo["path"]}" fill="{fg}" fill-rule="evenodd" transform="translate({wx:.3f} {wy:.3f}) scale({cap:.4f})"/></g>'
    s += (f'<text x="{ox:.1f}" y="{oy + L["h"] * k + sub_px * 1.9:.1f}" font-family="Noto Sans" font-size="{sub_px:.1f}" '
          f'fill="{"#A3A29C" if dark else "#6B6A64"}">{sub}</text>')
    return s


def png_banner(geo, w: int, h: int, **kw) -> Image.Image:
    inner = banner_svg(geo, w, h, **kw)
    html = (f'<!doctype html><html><head><meta charset="utf-8">{FONT_LINK}<style>html,body{{margin:0}}</style></head>'
            f'<body><svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{inner}</svg></body></html>')
    return render_html(html, w, h, transparent=False).convert("RGB")


# ---------- the start-up tone ----------
TONE_HZ, TONE_UNIT, TONE_RAMP, TONE_RATE = 700.0, 0.100, 0.005, 48000


def tone_wav(path: Path, code: str = ".--.", lead: float = 0.45, tail: float = 2.9):
    """The start-up tone: P in Morse, 700 Hz sine, 12 words per minute, peak -6 dBFS."""
    import wave
    total = lead + sum((TONE_UNIT if c == "." else 3 * TONE_UNIT) + TONE_UNIT for c in code) + tail
    out = np.zeros(int(total * TONE_RATE))
    t, r = lead, int(TONE_RAMP * TONE_RATE)
    ramp = 0.5 - 0.5 * np.cos(np.linspace(0, math.pi, r))
    for c in code:
        dur = TONE_UNIT if c == "." else 3 * TONE_UNIT
        n = int(dur * TONE_RATE)
        env = np.ones(n)
        env[:r], env[-r:] = ramp, ramp[::-1]
        i = int(t * TONE_RATE)
        out[i:i + n] += np.sin(2 * math.pi * TONE_HZ * np.arange(n) / TONE_RATE) * env
        t += dur + TONE_UNIT
    pcm = (out * 0.5 * 32767).astype("<i2")
    with wave.open(str(path), "wb") as wv:
        wv.setnchannels(2); wv.setsampwidth(2); wv.setframerate(TONE_RATE)
        wv.writeframes(np.repeat(pcm[:, None], 2, axis=1).tobytes())


# ---------- the wordmark ----------
def find_edge() -> str:
    for p in (r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
              r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
              r"C:\Program Files\Google\Chrome\Application\chrome.exe"):
        if Path(p).exists():
            return p
    raise SystemExit("the wordmark needs Edge or Chrome")


def render_html(html: str, width: int, height: int, transparent: bool = True) -> Image.Image:
    edge = find_edge()
    with tempfile.TemporaryDirectory() as tmp:
        page, shot = Path(tmp) / "p.html", Path(tmp) / "s.png"
        page.write_text(html, encoding="utf-8")
        args = [edge, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
                "--virtual-time-budget=8000", f"--window-size={width},{height}", f"--screenshot={shot}"]
        if transparent:
            args.append("--default-background-color=00000000")
        for attempt in range(3):  # a busy machine sometimes leaves Edge hanging: try again
            try:
                subprocess.run(args + [page.as_uri()], capture_output=True, timeout=120)
                if shot.exists():
                    break
            except subprocess.TimeoutExpired:
                pass  # run() has stopped that Edge; the next attempt starts a fresh one
        return Image.open(shot).convert("RGBA").copy()


FONT_LINK = ('<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500&'
             'family=Noto+Sans:wght@400;500;600&display=swap" rel="stylesheet">')


def text_mask(text: str, px: int = 600, weight: int = WEIGHT, family: str = FONT) -> tuple[np.ndarray, int]:
    """Alpha mask of the text and the baseline row. Black on transparent, line height 1."""
    pad = px // 4
    html = (f'<!doctype html><html><head><meta charset="utf-8">{FONT_LINK}<style>html,body{{margin:0;background:transparent}}'
            f'span{{font-family:"{family}";font-weight:{weight};font-size:{px}px;line-height:1;color:#000;'
            f'position:absolute;left:{pad}px;top:{pad}px;white-space:nowrap}}</style></head><body>'
            f'<span id="t">{text}</span><span style="left:{pad}px;top:{pad + 2 * px}px">H</span></body></html>')
    img = render_html(html, int(px * (len(text) * 0.75 + 1)), pad * 2 + 3 * px + pad)
    alpha = np.asarray(img)[..., 3]
    # both blocks start pad above their text, so a row in one is the same row in the other
    word = alpha[: 2 * px]
    h_row = alpha[2 * px:]
    hy = np.flatnonzero(h_row.max(axis=1) > 128)
    if word.max() < 200 or hy.size == 0:
        raise SystemExit("the wordmark render is empty: the font did not load")
    cap = hy[-1] - hy[0] + 1
    baseline = hy[-1] + 1
    return word, baseline, cap


def wordmark_geometry(px: int = 640):
    """Traced outline (SVG path, in cap units: cap height = 1) and the high-res mask."""
    word, baseline, cap = text_mask(WORD, px)
    ys, xs = np.nonzero(word > 8)
    x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
    crop = word[y0:y1, x0:x1]
    _, binary = cv2.threshold(crop, 127, 255, cv2.THRESH_BINARY)
    contours, _ = cv2.findContours(binary, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_NONE)
    parts = []
    base_off = baseline - y0  # baseline, from the top of the crop
    for c in contours:
        c = cv2.approxPolyDP(c, 0.35, True).reshape(-1, 2)
        if len(c) < 3:
            continue
        parts.append("M" + " L".join(f"{x / cap:.4f} {(y - base_off) / cap:.4f}" for x, y in c) + " Z")
    width_caps = (x1 - x0) / cap
    descent_caps = (y1 - y0 - base_off) / cap
    ascent_caps = base_off / cap
    return {"path": " ".join(parts), "width": width_caps, "ascent": ascent_caps, "descent": descent_caps,
            "mask": crop, "cap_px": cap, "base_px": base_off}


# ---------- logos ----------
def logo_layout(geo, stacked: bool = False, u: float = 20.0):
    """Placement of the mark and the wordmark; returns (inner builder, width, height)."""
    cap = CAP_UNITS * u
    if not stacked:
        mark_w, mark_h = 5 * u, 3 * u
        word_x = mark_w + GAP_UNITS * u
        top = max(mark_h, cap * geo["ascent"])
        height = top + cap * geo["descent"]
        base_y = top                      # baseline
        mark_y = base_y - cap / 2 - mark_h / 2
        return {"mark": (0, mark_y, u), "word": (word_x, base_y, cap),
                "w": word_x + cap * geo["width"], "h": max(height, mark_y + mark_h)}
    mu = u * STACK_MARK_SCALE
    mark_w, mark_h = 5 * mu, 3 * mu
    word_w = cap * geo["width"]
    w = max(mark_w, word_w)
    base_y = mark_h + GAP_UNITS * mu + cap * geo["ascent"]
    return {"mark": ((w - mark_w) / 2, 0, mu), "word": ((w - word_w) / 2, base_y, cap),
            "w": w, "h": base_y + cap * geo["descent"]}


def svg_logo(geo, dot: str, dash: str, text: str, stacked: bool = False, u: float = 20.0, label: str = "PanelOS") -> str:
    L = logo_layout(geo, stacked, u)
    mx, my, mu = L["mark"]
    wx, wy, cap = L["word"]
    inner = svg_elements(mu, mx, my, dot, dash)
    inner += (f'<path d="{geo["path"]}" fill="{text}" fill-rule="evenodd" '
              f'transform="translate({wx:.3f} {wy:.3f}) scale({cap:.4f})"/>')
    return svg_doc(inner, L["w"], L["h"], label)


# ---------- raster ----------
def draw_elements(draw: ImageDraw.ImageDraw, els, scale: float, dot, dash):
    for kind, x, y, w, h in els:
        box = [x * scale, y * scale, (x + w) * scale, (y + h) * scale]
        if kind == "dot":
            draw.ellipse(box, fill=dot)
        else:
            draw.rounded_rectangle(box, radius=h * scale / 2, fill=dash)


def png_icon(size: int, ground=INK, dot=SIGNAL, dash=PAPER) -> Image.Image:
    ss = 4
    S = size * ss
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pts = []
    for i in range(720):
        t = 2 * math.pi * i / 720
        c, s = math.cos(t), math.sin(t)
        R = S / 2 * ICON_INSET
        pts.append((S / 2 + R * math.copysign(abs(c) ** (2 / SQUIRCLE_N), c),
                    S / 2 + R * math.copysign(abs(s) ** (2 / SQUIRCLE_N), s)))
    d.polygon(pts, fill=hex_rgb(ground))
    u = size * ICON_MARK_WIDTH / 5
    draw_elements(d, mark_elements(u, (size - 5 * u) / 2, (size - 3 * u) / 2), ss, hex_rgb(dot), hex_rgb(dash))
    return img.resize((size, size), Image.LANCZOS)


def png_logo(geo, height_px: int, dot=SIGNAL, dash=INK, text=INK, stacked=False) -> Image.Image:
    L = logo_layout(geo, stacked, 20.0)
    scale = height_px / L["h"]
    ss = 4
    W, H = int(round(L["w"] * scale)), height_px
    img = Image.new("RGBA", (W * ss, H * ss), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    mx, my, mu = L["mark"]
    draw_elements(d, mark_elements(mu, mx, my), scale * ss, hex_rgb(dot), hex_rgb(dash))
    img = img.resize((W, H), Image.LANCZOS)
    # wordmark from the high-res mask
    wx, wy, cap = L["word"]
    k = cap * scale / geo["cap_px"]
    mask = Image.fromarray(geo["mask"]).resize((max(1, round(geo["mask"].shape[1] * k)), max(1, round(geo["mask"].shape[0] * k))), Image.LANCZOS)
    top = wy * scale - geo["base_px"] * k
    layer = Image.new("RGBA", mask.size, hex_rgb(text) + (0,))
    layer.putalpha(mask)
    img.alpha_composite(layer, (int(round(wx * scale)), int(round(top))))
    return img


def write(path: Path, text: str):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def main():
    geo = wordmark_geometry()
    (OUT / "wordmark.json").write_text(json.dumps({k: geo[k] for k in ("width", "ascent", "descent")}, indent=1))
    write(OUT / "wordmark-path.txt", geo["path"])
    for name in MARK_COLOURS:
        write(OUT / f"mark-{name}.svg", svg_mark(name))
    write(OUT / "app-icon.svg", svg_doc(icon_inner(), 512, 512, "PanelOS"))
    logos = {
        "logo": (SIGNAL, INK, INK, False), "logo-dark": (SIGNAL, PAPER, PAPER, False),
        "logo-stacked": (SIGNAL, INK, INK, True), "logo-stacked-dark": (SIGNAL, PAPER, PAPER, True),
        "logo-mono-ink": (INK, INK, INK, False), "logo-mono-white": ("#FFFFFF", "#FFFFFF", "#FFFFFF", False),
    }
    for name, (dot, dash, text, stacked) in logos.items():
        write(OUT / f"{name}.svg", svg_logo(geo, dot, dash, text, stacked))
    for name, (dot, dash) in {"signature": (SIGNAL, INK), "signature-dark": (SIGNAL, PAPER)}.items():
        inner, w = svg_signature(dot, dash)
        write(OUT / f"{name}.svg", svg_doc(inner, w, 10, "P O S"))
    for name, cols, ground in (("pattern-dark", PATTERN_DARK, GRAPHITE), ("pattern-light", PATTERN_LIGHT, PAPER)):
        inner = f'<rect width="1600" height="900" fill="{ground}"/>' + pattern_inner(1600, 900, 16, cols["dot"], cols["dash"])
        write(OUT / f"{name}.svg", svg_doc(inner, 1600, 900, "PanelOS pattern"))

    # Home Assistant: icon = the badge (works on light and dark), logo = mark + wordmark
    HA_BRAND.mkdir(parents=True, exist_ok=True)
    png_icon(256).save(HA_BRAND / "icon.png")
    png_icon(512).save(HA_BRAND / "icon@2x.png")
    png_logo(geo, 128).save(HA_BRAND / "logo.png")
    png_logo(geo, 256).save(HA_BRAND / "logo@2x.png")
    png_logo(geo, 128, SIGNAL, PAPER, PAPER).save(HA_BRAND / "dark_logo.png")
    png_logo(geo, 256, SIGNAL, PAPER, PAPER).save(HA_BRAND / "dark_logo@2x.png")

    # the kit: SVGs plus PNGs at common sizes
    # the kit is overwritten in place (Google Drive may hold a folder open, so it is not deleted)
    for sub in ("mark", "logo", "icon", "signature", "pattern", "social", "sound"):
        (KIT / sub).mkdir(parents=True, exist_ok=True)
    for name in MARK_COLOURS:
        shutil.copy(OUT / f"mark-{name}.svg", KIT / "mark")
    for name in logos:
        shutil.copy(OUT / f"{name}.svg", KIT / "logo")
    for name, (dot, dash, text, stacked) in logos.items():
        png_logo(geo, 240 if stacked else 120, dot, dash, text, stacked).save(KIT / "logo" / f"{name}.png")
        png_logo(geo, 480 if stacked else 240, dot, dash, text, stacked).save(KIT / "logo" / f"{name}@2x.png")
    shutil.copy(OUT / "app-icon.svg", KIT / "icon")
    for s in (16, 32, 48, 64, 128, 180, 256, 512, 1024):
        png_icon(s).save(KIT / "icon" / f"app-icon-{s}.png")
    for name in ("signature", "signature-dark"):
        shutil.copy(OUT / f"{name}.svg", KIT / "signature")
    for name in ("pattern-dark", "pattern-light"):
        shutil.copy(OUT / f"{name}.svg", KIT / "pattern")
    png_banner(geo, 1280, 320).save(KIT / "social" / "readme-header.png")
    png_banner(geo, 1280, 320, dark=False, sid="bsigl").save(KIT / "social" / "readme-header-light.png")
    png_banner(geo, 1280, 640, sub="Gives old smart home panels a second life", logo_frac=0.17).save(KIT / "social" / "social-preview.png")
    tone_wav(KIT / "sound" / "panelos-start-up.wav")
    readme = [
        "PanelOS brand kit",
        "=================",
        "",
        "The mark is the letter P in Morse code (dot, dash, dash, dot). Read the guidelines",
        "(PanelOS-Brand-Guidelines.pdf) before using these files.",
        "",
        "mark/       the mark: colour, colour on dark, one colour Ink, one colour white (SVG)",
        "logo/       logo and stacked logo, light and dark, one-colour versions (SVG, PNG 1x and 2x)",
        "icon/       the app icon (SVG, PNG from 16 to 1024 px)",
        "signature/  P O S in Morse on one line (SVG)",
        "pattern/    the pattern on dark and on light (SVG)",
        "social/     README header (dark, light) and social preview (PNG)",
        "sound/      the start-up tone (WAV, 48 kHz)",
        "motion/     the start-up as a 16:9 film with its tone, 3840x2160 and 1920x1080 (MP4, 60 fps)",
        "",
        "Colours: Signal #FF4A1C, Ink #101114, Paper #F5F3EE, Night #000000, Graphite #121314.",
        "Typefaces: Space Grotesk Medium (wordmark, headings), Noto Sans (text). SIL Open Font License.",
    ]
    nl = chr(10)
    (KIT / "README.txt").write_text(nl.join(readme) + nl, encoding="utf-8", newline=nl)
    shutil.copy(HERE / "PanelOS-Brand-Guidelines.pdf", KIT) if (HERE / "PanelOS-Brand-Guidelines.pdf").exists() else None
    zpath = HERE / "PanelOS-Brand-Kit.zip"
    with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED) as z:
        for f in sorted(KIT.rglob("*")):
            if f.is_file():
                z.write(f, Path("PanelOS-Brand-Kit") / f.relative_to(KIT))
    print("wordmark: width", round(geo["width"], 3), "caps; cap", geo["cap_px"], "px; kit:", zpath.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
