"""PanelOS brand guidelines: guidelines.html (A4 landscape pages) and the PDF.

Everything is drawn from the same geometry as the exported files
(branding/build_brand.py); the wordmark comes from branding/wordmark-path.txt,
so run build_brand.py first.

Run: python branding/guidelines/build_guidelines.py   (needs Edge for the PDF)
"""

from __future__ import annotations

import datetime as dt
import json
import math
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BRANDING = HERE.parent
sys.path.insert(0, str(BRANDING))
import build_brand as bb  # noqa: E402

WORD_PATH = (BRANDING / "wordmark-path.txt").read_text(encoding="utf-8").strip()
WORD = json.loads((BRANDING / "wordmark.json").read_text(encoding="utf-8"))
GEO = {"path": WORD_PATH, **WORD}
VERSION = "1.0"
DATE = dt.date.today().strftime("%B %Y")

SIGNAL, INK, PAPER, NIGHT, GRAPHITE = bb.SIGNAL, bb.INK, bb.PAPER, bb.NIGHT, bb.GRAPHITE
MUTED = "#6B6A64"
LINE = "#DCD9D0"

# ---------------------------------------------------------------- helpers


def svg(inner: str, w: float, h: float, width: str | None = None, height: str | None = None, extra: str = "") -> str:
    size = (f' width="{width}"' if width else "") + (f' height="{height}"' if height else "")
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.2f} {h:.2f}"{size} {extra}>{inner}</svg>'


def mark(dot=SIGNAL, dash=INK, u=20.0, x=0.0, y=0.0) -> str:
    return bb.svg_elements(u, x, y, dot, dash)


def mark_svg(dot=SIGNAL, dash=INK, width="40mm") -> str:
    return svg(mark(dot, dash), 100, 60, width=width)


def logo_inner(dot=SIGNAL, dash=INK, text=INK, stacked=False, u=20.0, x=0.0, y=0.0):
    L = bb.logo_layout(GEO, stacked, u)
    mx, my, mu = L["mark"]
    wx, wy, cap = L["word"]
    inner = bb.svg_elements(mu, x + mx, y + my, dot, dash)
    inner += (f'<path d="{WORD_PATH}" fill="{text}" fill-rule="evenodd" '
              f'transform="translate({x + wx:.3f} {y + wy:.3f}) scale({cap:.4f})"/>')
    return inner, L["w"], L["h"]


def logo_svg(dot=SIGNAL, dash=INK, text=INK, stacked=False, width=None, height=None) -> str:
    inner, w, h = logo_inner(dot, dash, text, stacked)
    return svg(inner, w, h, width=width, height=height)


def icon_svg(width="30mm", ground=INK) -> str:
    return svg(bb.icon_inner(512, ground), 512, 512, width=width)


_IDS = {"n": 0}


def uid(prefix="p") -> str:
    _IDS["n"] += 1
    return f"{prefix}{_IDS['n']}"


def pattern_svg(w, h, u, cols, ground, phase=0, width="100%", height="100%", fit="xMidYMid slice") -> str:
    inner = f'<rect width="{w}" height="{h}" fill="{ground}"/>' + bb.pattern_inner(w, h, u, cols["dot"], cols["dash"], phase, uid())
    return svg(inner, w, h, width=width, height=height, extra=f'preserveAspectRatio="{fit}"')


def lum(hex_colour: str) -> float:
    def ch(c):
        c /= 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = bb.hex_rgb(hex_colour)
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)


def contrast(a: str, b: str) -> float:
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def cmyk(hex_colour: str) -> str:
    r, g, b = (c / 255 for c in bb.hex_rgb(hex_colour))
    k = 1 - max(r, g, b)
    if k >= 1:
        return "0 0 0 100"
    c, m, y = ((1 - x - k) / (1 - k) for x in (r, g, b))
    return " ".join(str(round(v * 100)) for v in (c, m, y, k))


PAGES: list[str] = []
SECTION = {"name": ""}


def page(body: str, cls: str = "", section: str | None = None):
    sec = SECTION["name"] if section is None else section
    n = len(PAGES) + 1
    chrome = ""
    if "cover" not in cls and "divider" not in cls:
        chrome = (f'<div class="hdr"><span>PanelOS brand guidelines</span><span>{sec}</span></div>'
                  f'<div class="ftr"><span>Version {VERSION} · {DATE}</span><span>{n}</span></div>')
    PAGES.append(f'<section class="page {cls}">{chrome}{body}</section>')


def divider(num: str, title: str, note: str, items: list[str]):
    SECTION["name"] = title
    bg = pattern_svg(1188, 840, 22, bb.PATTERN_DARK, NIGHT, phase=int(num) * 3)
    lis = "".join(f"<li>{i}</li>" for i in items)
    page(f'<div class="bgfill">{bg}</div><div class="dv"><div class="dv-num">{num}</div>'
         f'<div class="dv-box"><div class="dv-title">{title}</div><div class="dv-note">{note}</div>'
         f'<ul class="dv-list">{lis}</ul></div></div>', "divider")


# ---------------------------------------------------------------- pages

def cover():
    bg = pattern_svg(1188, 840, 22, bb.PATTERN_DARK, NIGHT)
    lg = logo_svg(SIGNAL, PAPER, PAPER, width="150mm")
    page(f'<div class="bgfill">{bg}</div><div class="cv-calm"></div><div class="cv-logo">{lg}</div>'
         f'<div class="cv-title">Brand guidelines</div>'
         f'<div class="cv-sub">The mark, its colours, the pattern, motion and sound, type and voice,<br>'
         f'and how PanelOS appears on the panel, in Home Assistant and on the web.<br>'
         f'<span class="cv-ver">Version {VERSION} · {DATE}</span></div>', "cover", section="")


def contents():
    SECTION["name"] = "Contents"
    rows = [
        ("01", "The idea", "A letter in light, four principles"),
        ("02", "The mark", "Construction, forms, space and size, colour versions, misuse"),
        ("03", "Colour", "The palette, screen and print values, proportion and contrast"),
        ("04", "The pattern", "How it is built and where it goes"),
        ("05", "Motion and sound", "The start-up sequence and its tone"),
        ("06", "Type and voice", "Typefaces, the name, how PanelOS speaks"),
        ("07", "In use", "On the panel, Home Assistant, GitHub and the web, files, the reasons"),
    ]
    items = "".join(f'<div class="toc-row"><span class="toc-n">{n}</span><span class="toc-t">{t}</span>'
                    f'<span class="toc-d">{d}</span></div>' for n, t, d in rows)
    page(f'<div class="kicker">PanelOS</div><h1>One look, wherever PanelOS appears</h1>'
         f'<div class="cols2"><div><p class="lead">PanelOS is open firmware for smart home wall panels. It replaces the '
         f'closed software a panel came with by a small Linux system with a fast native Home Assistant dashboard, an '
         f'assistant screen and a voice satellite. No cloud, nothing the panel did not need.</p>'
         f'<p>These guidelines describe one identity for everything that carries the name: the panel\'s own screens, '
         f'the Home Assistant integration, the repository, the documentation and the website. Every file mentioned '
         f'here is in the brand kit; every measure comes from the same geometry as those files.</p></div>'
         f'<div class="toc">{items}</div></div>')


def idea():
    SECTION["name"] = "The idea"
    big = svg(mark(SIGNAL, INK, 40), 200, 120, width="88mm")
    code = ('<div class="codeline"><span class="dotc"></span><span class="dashc"></span><span class="dashc"></span>'
            '<span class="dotc"></span><b>P</b></div>')
    page(f'<div class="kicker">The idea</div><h1>A letter in light</h1>'
         f'<div class="cols2 mid"><div class="figure">{big}</div><div>'
         f'<p class="lead">The mark is the letter P in Morse code: dot, dash, dash, dot. Set in two rows it becomes a '
         f'sign of its own, and it reads the same after a half turn, because the code is the same from either end.</p>'
         f'{code}'
         f'<p>Morse code is a signal that is only on or off, light or sound, and one of the oldest ways of sending text. '
         f'PanelOS lives in that world: a screen in the wall that lights up when someone comes near, listens, and answers.</p>'
         f'<p>Look again and the mark is also a small dashboard: two small cards and two wide ones in two rows, the kind '
         f'of screen PanelOS draws.</p>'
         f'<p>And it tells where PanelOS comes from. Many panels are left with software that was closed, forgotten or '
         f'of little use. PanelOS gives them a second life, and on the screen the code lights up one element after '
         f'the other, like embers catching.</p></div></div>')


def principles():
    cards = [
        ("01", "Signal, not noise", "Four elements, two colours, plenty of dark around them. Nothing is added to the mark, "
                                     "and the screens around it stay as calm as it is."),
        ("02", "Code you can read", "The mark is not a picture of something: it is the letter itself, written in an open "
                                     "code anyone can look up. Open like the firmware."),
        ("03", "Calm on the wall", "PanelOS lives in homes, in kitchens and halls, at night too. Dark grounds, no glare, "
                                    "red only where something is lit."),
        ("04", "A second life", "Devices that were set aside get to work again. The identity is quiet and durable, "
                                 "made to last as long as the panels it runs on."),
    ]
    html = "".join(f'<div class="pcard"><div class="pnum">{n}</div><div class="ptitle">{t}</div><p>{d}</p></div>'
                   for n, t, d in cards)
    page(f'<div class="kicker">The idea</div><h1>Four principles</h1><div class="grid4">{html}</div>')


def construction():
    SECTION["name"] = "The mark"
    u = 40
    ox, oy = 60, 50
    grid = []
    for i in range(6):
        grid.append(f'<line x1="{ox + i * u}" y1="{oy - 14}" x2="{ox + i * u}" y2="{oy + 3 * u + 14}" stroke="{LINE}" stroke-width="1"/>')
    for j in range(4):
        grid.append(f'<line x1="{ox - 14}" y1="{oy + j * u}" x2="{ox + 5 * u + 14}" y2="{oy + j * u}" stroke="{LINE}" stroke-width="1"/>')
    dims = (
        f'<text x="{ox + u / 2}" y="{oy - 22}" class="dim">u</text>'
        f'<text x="{ox + 3.5 * u}" y="{oy - 22}" class="dim">3u</text>'
        f'<text x="{ox + 1.5 * u}" y="{oy - 22}" class="dim">u</text>'
        f'<text x="{ox + 2.5 * u}" y="{oy + 3 * u + 34}" class="dim">5u</text>'
        f'<text x="{ox - 30}" y="{oy + 1.5 * u + 5}" class="dim">3u</text>'
        f'<text x="{ox + 5 * u + 30}" y="{oy + 1.5 * u + 5}" class="dim">u</text>'
        f'<circle cx="{ox + 2.5 * u}" cy="{oy + 1.5 * u}" r="4" fill="{MUTED}"/>'
    )
    diagram = svg("".join(grid) + mark(SIGNAL, INK, u, ox, oy) + dims, 320, 260, width="118mm")
    page(f'<div class="kicker">The mark</div><h1>Built on one unit</h1><div class="cols2 mid"><div class="figure">{diagram}</div><div>'
         f'<p class="lead">Everything is measured in one unit, u: the height of a dash and the width of a dot.</p>'
         f'<ul class="bul"><li>A dot is a circle u wide. A dash is 3u long and u high, with fully round ends: the '
         f'proportions of Morse code itself.</li>'
         f'<li>Elements in a row stand u apart; the two rows stand u apart. The mark is 5u wide and 3u high.</li>'
         f'<li>The top row is dot and dash, the bottom row dash and dot. Read across, top to bottom, that is the letter P: '
         f'<b>· − − ·</b>.</li><li>The mark is symmetric about its centre (the grey point). Turned half a turn it is the '
         f'same; it is never turned a quarter turn, mirrored or tilted.</li>'
         f'<li>Dots are always Signal red: they are the lit part of the code. Dashes take the text colour of the ground: '
         f'Ink on light, Paper or white on dark.</li></ul></div></div>')


def forms():
    cells = [
        (svg(mark(SIGNAL, INK), 100, 60, width="26mm"), "Mark", "Where the name is already present or space is small: favicons, "
         "the panel's status line, small print."),
        (icon_svg("24mm"), "App icon", "The mark on an Ink badge with continuous corners (a superellipse). Home Assistant's "
         "icon, avatars, launchers."),
        (logo_svg(width="58mm"), "Logo", "Mark and wordmark side by side: the first choice everywhere else."),
        (logo_svg(stacked=True, width="36mm"), "Stacked logo", "For square or narrow spaces: covers, the panel's boot screen."),
    ]
    sig, w = bb.svg_signature(SIGNAL, INK, 10)
    cells.append((svg(sig, w, 10, width="44mm"), "Signature", "The whole short name, P O S, in Morse on one line. A rule or a "
                  "closing line, never instead of the logo."))
    html = "".join(f'<div class="fcell"><div class="fart">{a}</div><div class="ft">{t}</div><p class="small">{d}</p></div>'
                   for a, t, d in cells)
    page(f'<div class="kicker">The mark</div><h1>Five forms, one look</h1><div class="grid5 forms">{html}</div>'
         f'<p class="small note">The logo\'s wordmark is Space Grotesk Medium. Its cap height is 4u, so the mark is three '
         f'quarters of it, and the mark stands 2u before the P with its centre on the middle of the cap height. Stacked, the '
         f'mark is 1.6 times larger and centred 3.2u above the cap line. These proportions were chosen in use; they are not '
         f'changed.</p>')


def space_size():
    u = 24
    inner, w, h = logo_inner(u=u)
    pad = 2 * u
    W, H = w + 2 * pad, h + 2 * pad
    fig = (f'<rect x="0" y="0" width="{W}" height="{H}" fill="#FCE6DE"/>'
           f'<rect x="{pad}" y="{pad}" width="{w}" height="{h}" fill="#FFFFFF"/>'
           f'<rect x="0.75" y="0.75" width="{W - 1.5}" height="{H - 1.5}" fill="none" stroke="{SIGNAL}" stroke-dasharray="6 5" stroke-width="1.5"/>')
    for x1, y1, x2, y2, tx, ty in ((0, H / 2, pad, H / 2, pad / 2, H / 2 - 6), (pad + w, H / 2, W, H / 2, pad + w + pad / 2, H / 2 - 6),
                                    (W / 2, 0, W / 2, pad, W / 2 + 14, pad / 2 + 4), (W / 2, pad + h, W / 2, H, W / 2 + 14, pad + h + pad / 2 + 4)):
        fig += (f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{SIGNAL}" stroke-width="1.2"/>'
                f'<text x="{tx}" y="{ty}" class="dim" text-anchor="middle">2u</text>')
    inner2, _, _ = logo_inner(u=u, x=pad, y=pad)
    clear_svg = svg(fig + inner2, W, H, width="122mm")
    um = 30
    mfig = (f'<rect width="{9 * um}" height="{7 * um}" fill="#FCE6DE"/><rect x="{2 * um}" y="{2 * um}" width="{5 * um}" height="{3 * um}" fill="#fff"/>'
            + mark(SIGNAL, INK, um, 2 * um, 2 * um))
    sizes = "".join(f'<div class="szc">{svg(mark(SIGNAL, INK), 100, 60, width=f"{s}px")}<span>{s} px</span></div>'
                    for s in (64, 32, 24, 16))
    page(f'<div class="kicker">The mark</div><h1>Room to breathe, and how small</h1><div class="cols2"><div>'
         f'<div class="figure">{clear_svg}</div><div class="figure" style="margin-top:6mm">{svg(mfig, 9 * um, 7 * um, width="40mm")}</div>'
         f'<p style="margin-top:5mm">Keep at least 2u free on every side of the logo and the mark, where u is the mark&#8217;s unit at the size '
         f'used (the tinted area). Nothing enters that space: no text, no edge, no other logo, no busy part of the pattern.</p></div>'
         f'<div><div class="szrow">{sizes}</div>'
         f'<table class="tbl"><tr><th></th><th>Screen</th><th>Print</th></tr>'
         f'<tr><td>Mark</td><td>16 px wide</td><td>6 mm wide</td></tr>'
         f'<tr><td>App icon</td><td>16 px</td><td>8 mm</td></tr>'
         f'<tr><td>Logo</td><td>96 px wide</td><td>25 mm wide</td></tr>'
         f'<tr><td>Stacked logo</td><td>64 px wide</td><td>18 mm wide</td></tr></table>'
         f'<p class="small">Below these sizes the dots and dashes run together. At 16 px the mark still reads; the kit&#8217;s small '
         f'PNGs are drawn for those sizes.</p></div></div>')


def colour_versions():
    cells = [
        (PAPER, svg(mark(SIGNAL, INK), 100, 60, width="40mm"), "Colour on light", "Signal dots, Ink dashes. The first choice."),
        (GRAPHITE, svg(mark(SIGNAL, PAPER), 100, 60, width="40mm"), "Colour on dark", "Signal dots, Paper dashes (white on the panel)."),
        ("#FFFFFF", svg(mark(INK, INK), 100, 60, width="40mm"), "One colour, Ink", "Where only one colour prints: rubber stamps, engraving, laser marking."),
        (NIGHT, svg(mark("#FFFFFF", "#FFFFFF"), 100, 60, width="40mm"), "One colour, white", "On photos and dark single-colour grounds."),
    ]
    html = "".join(f'<div class="vcell"><div class="vart" style="background:{bg}">{a}</div><div class="ft">{t}</div>'
                   f'<p class="small">{d}</p></div>' for bg, a, t, d in cells)
    page(f'<div class="kicker">The mark</div><h1>Four colour versions</h1><div class="grid4">{html}</div>'
         f'<p class="small note">There is no version on a Signal red ground: the dots would disappear. Grey tints, gradients '
         f'and two shades of red are not versions either.</p>')


def misuse():
    """Eight wrong uses, each drawn at the same size in the same frame."""
    U = 22
    BW, BH = 240, 140                       # every example sits centred in this box

    def boxed(inner, w, h):
        return svg(f'<g transform="translate({(BW - w) / 2:.2f} {(BH - h) / 2:.2f})">{inner}</g>', BW, BH, width="100%")

    mw, mh = 5 * U, 3 * U
    swapped = bb.svg_elements(U, 0, 0, INK, SIGNAL)
    order = (f'<rect x="0" y="0" width="{3 * U}" height="{U}" rx="{U / 2}" fill="{INK}"/>'
             f'<circle cx="{4.5 * U}" cy="{U / 2}" r="{U / 2}" fill="{SIGNAL}"/>'
             f'<circle cx="{U / 2}" cy="{2.5 * U}" r="{U / 2}" fill="{SIGNAL}"/>'
             f'<rect x="{2 * U}" y="{2 * U}" width="{3 * U}" height="{U}" rx="{U / 2}" fill="{INK}"/>')
    turned = f'<g transform="translate({mh} 0) rotate(90)">{mark(SIGNAL, INK, U)}</g>'
    stretched = f'<g transform="scale(1.55 0.8)">{mark(SIGNAL, INK, U)}</g>'
    outlined = "".join(
        (f'<circle cx="{x + w / 2}" cy="{y + h / 2}" r="{w / 2 - 1.5}" fill="none" stroke="{SIGNAL}" stroke-width="3"/>' if k == "dot" else
         f'<rect x="{x + 1.5}" y="{y + 1.5}" width="{w - 3}" height="{h - 3}" rx="{h / 2 - 1.5}" fill="none" stroke="{INK}" stroke-width="3"/>')
        for k, x, y, w, h in bb.mark_elements(U))
    fx_id = uid("fx")
    effects = (f'<defs><filter id="{fx_id}" x="-30%" y="-40%" width="160%" height="190%"><feDropShadow dx="3" dy="5" stdDeviation="3.5" '
               f'flood-color="#000" flood-opacity=".40"/></filter><linearGradient id="{fx_id}g" x1="0" y1="0" x2="0" y2="1">'
               f'<stop offset="0" stop-color="#5A5C63"/><stop offset="1" stop-color="{INK}"/></linearGradient></defs>'
               f'<g filter="url(#{fx_id})">{mark(SIGNAL, f"url(#{fx_id}g)", U)}</g>')
    recoloured = bb.svg_elements(U, 0, 0, "#2E9E5B", "#2B4BFF")
    # the real lockup's proportions, but the wordmark set in a serif
    su = U * 0.36
    cap = 4 * su
    serif = (mark(SIGNAL, INK, su, 0, (cap - 3 * su) / 2) +
             f'<text x="{7 * su:.1f}" y="{cap:.1f}" font-family="Georgia, serif" '
             f'font-style="italic" font-size="{cap / 0.69:.1f}" fill="{INK}">PanelOS</text>')
    cells = [
        (boxed(swapped, mw, mh), "Swapped colours", "The dots are always Signal: they are the lit part of the code."),
        (boxed(order, mw, mh), "Changed order", "Dash and dot first would no longer read as P."),
        (boxed(turned, mh, mw), "Turned", "Never a quarter turn, a mirror image or a tilt. A half turn is the same mark."),
        (boxed(stretched, mw * 1.55, mh * 0.8), "Stretched", "Never stretched or squashed: dots stay round, dashes 3 units long."),
        (boxed(outlined, mw, mh), "Outlined", "Always solid. No outlines, no hollow versions."),
        (boxed(effects, mw, mh), "Effects", "No shadows, gradients or bevels. The glow belongs to the start-up only."),
        (boxed(recoloured, mw, mh), "Other colours", "Signal and Ink, Paper or white. No other colours, even for a theme."),
        (boxed(serif, 7 * su + cap / 0.69 * 3.55, cap + 2), "Other typeface", "The wordmark is the outlined Space Grotesk file, never retyped."),
    ]
    html = "".join(f'<div class="mc"><div class="mc-art">{a}<span class="mc-x"></span></div>'
                   f'<div class="mc-t">{t}</div><p class="small">{d}</p></div>' for a, t, d in cells)
    ok = svg(mark(SIGNAL, INK, U), mw, mh, width="22mm")
    page(f'<div class="kicker">The mark</div><h1>What not to do</h1>'
         f'<div class="mc-ok"><span class="mc-okart">{ok}</span><span>The mark as it is: two Signal dots, two dashes in the '
         f'ground\'s text colour, in this order, upright. Everything below changes one of these.</span></div>'
         f'<div class="grid4 mgrid2">{html}</div>')

def palette():
    SECTION["name"] = "Colour"
    cols = [
        ("Signal", SIGNAL, "The dots, and anything that is lit or needs a glance. Never body text."),
        ("Ink", INK, "Dashes and text on light grounds; the app icon's badge."),
        ("Paper", PAPER, "Light grounds; dashes and text on dark grounds."),
        ("Night", NIGHT, "The panel's own screen: start-up, screensaver."),
        ("Graphite", GRAPHITE, "Dark grounds in documents and on the web."),
    ]
    html = ""
    for name, hx, role in cols:
        r, g, b = bb.hex_rgb(hx)
        fg = INK if lum(hx) > 0.4 else PAPER
        html += (f'<div class="sw"><div class="swc" style="background:{hx};color:{fg}"><span>{name}</span></div>'
                 f'<div class="swv"><b>{hx}</b><br>RGB {r} {g} {b}<br>CMYK {cmyk(hx)}</div><p class="small">{role}</p></div>')
    pat = "".join(f'<div class="sw sm"><div class="swc" style="background:{v}"></div><div class="swv"><b>{v}</b><br>{k}</div></div>'
                  for k, v in (("Ember: pattern dot on dark", bb.PATTERN_DARK["dot"]), ("Ash: pattern dash on dark", bb.PATTERN_DARK["dash"]),
                               ("Blush: pattern dot on light", bb.PATTERN_LIGHT["dot"]), ("Stone: pattern dash on light", bb.PATTERN_LIGHT["dash"])))
    page(f'<div class="kicker">Colour</div><h1>The palette</h1><div class="grid5">{html}</div>'
         f'<div class="sub" style="margin-top:7mm">Pattern colours</div><div class="grid4 pgrid">{pat}</div>'
         f'<p class="small note">CMYK values are a starting point for print, converted without a colour profile: check them '
         f'against a proof. On screen the HEX values are the reference.</p>')


def proportion():
    rows = [(INK, PAPER, "Ink on Paper", "text"), (PAPER, GRAPHITE, "Paper on Graphite", "text"),
            ("#FFFFFF", NIGHT, "White on Night", "text"), (SIGNAL, PAPER, "Signal on Paper", "graphics"),
            (SIGNAL, GRAPHITE, "Signal on Graphite", "graphics"), (SIGNAL, NIGHT, "Signal on Night", "graphics")]
    trs = ""
    for fg, bg, name, use in rows:
        c = contrast(fg, bg)
        verdict = ("text at any size" if c >= 7 else "text at any size (AA)" if c >= 4.5 else
                   "large text and graphics" if c >= 3 else "decoration only")
        trs += (f'<tr><td><span class="chip" style="background:{bg};color:{fg}">Aa ●</span></td><td>{name}</td>'
                f'<td>{c:.1f} : 1</td><td>{verdict}</td></tr>')
    bar = (f'<div class="pbar"><span style="flex:46;background:{NIGHT}"></span><span style="flex:30;background:{PAPER};'
           f'border:1px solid {LINE}"></span><span style="flex:18;background:{INK}"></span><span style="flex:5;'
           f'background:{SIGNAL}"></span></div><div class="pbar-l"><span>Night / Graphite</span><span>Paper</span>'
           f'<span>Ink</span><span>Signal</span></div>')
    page(f'<div class="kicker">Colour</div><h1>Proportion and contrast</h1><div class="cols2"><div>'
         f'<p class="lead">Mostly dark and light, a little red. Signal is the colour of a lit element: about one part in twenty.</p>'
         f'{bar}<p>Signal marks what is on: the dots, an active state, a single highlighted number. It is never a ground for '
         f'text and never used for body text; for links and buttons the text colour stays Ink or Paper.</p></div><div>'
         f'<table class="tbl wide"><tr><th></th><th>Pair</th><th>Contrast</th><th>WCAG fit</th></tr>{trs}</table>'
         f'<p class="small">Contrast as defined by WCAG 2.2. Signal on light grounds stays below 4.5 : 1, so it is kept to '
         f'graphics and large text.</p></div></div>')


def pattern_build():
    SECTION["name"] = "The pattern"
    u = 14
    w, h = 560, 230
    demo = pattern_svg(w, h, u, {"dot": SIGNAL, "dash": INK}, PAPER, width="100%", height="62mm")
    sig_u = 12
    sig, sw = bb.svg_signature(SIGNAL, INK, sig_u)
    labels = (f'<text x="{5.5 * sig_u}" y="{-8}" class="dim" text-anchor="middle">P</text>'
              f'<text x="{19.5 * sig_u}" y="{-8}" class="dim" text-anchor="middle">O</text>'
              f'<text x="{30.5 * sig_u}" y="{-8}" class="dim" text-anchor="middle">S</text>')
    row = svg(f'<g transform="translate(0 22)">{labels}{sig}</g>', sw, 40, width="120mm")
    page(f'<div class="kicker">The pattern</div><h1>The code as texture</h1><div class="cols2"><div>'
         f'<p class="lead">The pattern repeats the short name, P O S, in Morse, row after row.</p><div class="figure">{row}</div>'
         f'<ul class="bul"><li>Elements as in the mark: dot u, dash 3u, u between elements, 3u between letters, 3u between '
         f'two repeats.</li><li>Rows are 2.6u apart, and each row starts 7u further to the left, so no column lines up.</li>'
         f'<li>It is always quiet: on dark grounds Ember dots and Ash dashes, on light grounds Blush dots and Stone dashes. '
         f'Full colour, as in the sample here, only to explain the construction.</li></ul></div>'
         f'<div class="figure framed">{demo}</div></div>')


def pattern_use():
    boot = boot_screen_svg("56mm")
    head = readme_header_svg("100%")
    page(f'<div class="kicker">The pattern</div><h1>Where it goes</h1><div class="cols2"><div>'
         f'<ul class="bul"><li><b>Yes:</b> covers and dividers, the README header, the social preview, the website\'s first '
         f'screen, the panel\'s start-up, the back of printed matter.</li>'
         f'<li><b>A calm area</b> behind the logo: the pattern stops at least 2u around it, or a ground-coloured panel '
         f'covers it.</li><li><b>Scale:</b> the pattern\'s unit is at most a quarter of the mark\'s unit next to it, so the '
         f'mark is always the largest piece of code on the page.</li>'
         f'<li><b>No:</b> behind running text, tables or form fields; in Signal and Ink at full strength; animated in the '
         f'background of a working screen.</li></ul></div><div class="stackfig"><div class="figure">{boot}</div>'
         f'<div class="figure framed">{head}</div></div></div>')


def boot_screen_svg(width):
    s = f'<rect width="480" height="480" fill="{NIGHT}"/>' + f'<g opacity="0.55">{bb.pattern_inner(480, 480, 12, bb.PATTERN_DARK["dot"], bb.PATTERN_DARK["dash"], 1, uid())}</g>'
    s += f'<rect x="88" y="120" width="304" height="250" rx="22" fill="{NIGHT}"/>'
    inner, w, h = logo_inner(SIGNAL, "#FFFFFF", "#FFFFFF", stacked=True)
    k = 240 / w
    s += f'<g transform="translate({240 - w * k / 2:.1f} {245 - h * k / 2:.1f}) scale({k:.4f})">{inner}</g>'
    return f'<div class="panelframe">{svg(s, 480, 480, width=width)}</div>'


def readme_header_svg(width, dark=True):
    return svg(bb.banner_svg(GEO, 1280, 320, dark=dark, sid=uid("b")), 1280, 320, width=width)


def social_svg(width):
    return svg(bb.banner_svg(GEO, 1280, 640, sub="Gives old smart home panels a second life", logo_frac=0.17, sid=uid("b")),
               1280, 640, width=width)


def motion():
    SECTION["name"] = "Motion and sound"
    unit = 0.10
    t0 = 0.45
    els = []
    t = t0
    for kind in ".--.":
        dur = unit if kind == "." else 3 * unit
        els.append((kind, t, dur))
        t += dur + unit
    t_word, t_fade, t_end = t - unit + 0.15, t - unit + 2.3, t - unit + 2.9
    scale = 205 / t_end  # mm per second
    lanes = [("Pattern", 0.0, 0.5, "fades in to 45 %"), ("Dot", els[0][1], 0.16, "pops in, a glow fades over 0.9 s"),
             ("Dash", els[1][1], els[1][2], "grows with its tone"), ("Dash", els[2][1], els[2][2], "grows with its tone"),
             ("Dot", els[3][1], 0.16, "pops in"), ("Name", t_word, 0.45, "fades in, rises 14 px"),
             ("Everything", t_fade, 0.6, "fades out")]
    rows = ""
    for i, (name, start, dur, note) in enumerate(lanes):
        col = SIGNAL if name == "Dot" else INK if name in ("Dash", "Name") else MUTED
        rows += (f'<div class="lane"><span class="ln">{name}</span><div class="lt"><span class="lb" style="left:{start * scale:.1f}mm;'
                 f'width:{max(dur * scale, 1.2):.1f}mm;background:{col}"></span><span class="lnote" style="left:{(start + dur) * scale + 2:.1f}mm">{note}</span></div></div>')
    ticks = "".join(f'<span class="tick" style="left:{x * scale:.1f}mm">{x:.1f} s</span>' for x in (0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0) if x <= t_end)
    frames = ""
    for k in range(0, 6):
        s = f'<rect width="480" height="480" fill="{NIGHT}"/>'
        s += f'<g opacity="{0.45 if k else 0.25}">{bb.pattern_inner(480, 480, 12, bb.PATTERN_DARK["dot"], bb.PATTERN_DARK["dash"], 1, uid())}</g>'
        u = 56
        x0, y0 = 240 - 2.5 * u, 196 - 1.5 * u
        for i, (kind, x, y, w, h) in enumerate(bb.mark_elements(u, x0, y0)):
            if i < k:
                s += (f'<circle cx="{x + w / 2}" cy="{y + h / 2}" r="{w / 2}" fill="{SIGNAL}"/>' if kind == "dot"
                      else f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{h / 2}" fill="#FFFFFF"/>')
        if k == 5:
            cap = 46
            s += (f'<path d="{WORD_PATH}" fill="#FFFFFF" fill-rule="evenodd" '
                  f'transform="translate({240 - GEO["width"] * cap / 2:.1f} {318 + cap:.1f}) scale({cap})"/>')
        frames += f'<div class="kf">{svg(s, 480, 480, width="27mm")}<span>{["0.2 s", "0.5 s", "0.8 s", "1.2 s", "1.5 s", "2.0 s"][k]}</span></div>'
    page(f'<div class="kicker">Motion and sound</div><h1>The start-up</h1>'
         f'<p class="lead">While the panel starts, the mark writes itself in Morse: one element per beat, each with its '
         f'tone, then the name. About four and a half seconds in all.</p><div class="kfrow">{frames}</div>'
         f'<div class="timeline"><div class="ticks">{ticks}</div>{rows}</div>'
         f'<p class="small note">Timing follows the code: one unit is 100 ms (12 words per minute), a dot lasts one unit, a dash '
         f'three, with one unit between elements. Easing is a cubic ease-out; nothing bounces. The start-up is the only '
         f'place where the mark moves or glows.</p>')


def sound():
    unit_mm = 8.5
    blocks = ""
    x = 0
    for kind in ".--.":
        w = unit_mm if kind == "." else 3 * unit_mm
        blocks += (f'<span class="sblk" style="left:{x}mm;width:{w}mm"></span>'
                   f'<span class="slbl" style="left:{x}mm;width:{w}mm">{"100 ms" if kind == "." else "300 ms"}</span>')
        x += w + unit_mm
    wave = []
    for i in range(0, 400):
        t = i / 400
        env = min(1, t / 0.05, (1 - t) / 0.05)
        wave.append(f"{i * 0.5:.1f} {20 - 16 * env * math.sin(2 * math.pi * 18 * t):.2f}")
    wave_svg = svg(f'<polyline points="{" ".join(wave)}" fill="none" stroke="{INK}" stroke-width="1"/>', 200, 40, width="80mm")
    page(f'<div class="kicker">Motion and sound</div><h1>The tone</h1><div class="cols2"><div>'
         f'<p class="lead">A classic Morse tone: a clean sine at 700 Hz, keyed in the code\'s own rhythm.</p>'
         f'<div class="srow">{blocks}</div>'
         f'<table class="tbl"><tr><td>Tone</td><td>Sine, 700 Hz, the same pitch for dots and dashes</td></tr>'
         f'<tr><td>Speed</td><td>12 words per minute: one unit is 100 ms</td></tr>'
         f'<tr><td>Keying</td><td>5 ms raised-cosine rise and fall: no clicks</td></tr>'
         f'<tr><td>Level</td><td>Peak at −6 dBFS; the panel\'s own volume applies</td></tr>'
         f'<tr><td>File</td><td>48 kHz, 16 bit, stereo WAV in the kit</td></tr></table></div><div>'
         f'<div class="figure">{wave_svg}</div><p class="small center">One dot, enlarged: the soft start and end are '
         f'the keying.</p><ul class="bul"><li>Only at start-up. Notifications, timers and the voice assistant have sounds of '
         f'their own; the tone stays special.</li><li>Never faster, slower, higher or with effects: a tone that is not '
         f'Morse is noise.</li><li>Where sound is not wanted (night, muted), the animation runs without it.</li></ul>'
         f'</div></div>')


def typefaces():
    SECTION["name"] = "Type and voice"
    page(f'<div class="kicker">Type and voice</div><h1>Two typefaces</h1><div class="cols2"><div class="tface">'
         f'<div class="tname g">Space Grotesk</div><div class="tsample g">Aa Bb Cc 0123</div>'
         f'<p>Medium (500) for the wordmark, headings and numbers that matter. Its round shapes with the odd sharp turn '
         f'sit well next to dots and dashes. Tracking normal; headings slightly tighter (−0.01 em).</p>'
         f'<p class="small">SIL Open Font License, from Google Fonts.</p></div><div class="tface">'
         f'<div class="tname n">Noto Sans</div><div class="tsample n">Aa Bb Cc 0123</div>'
         f'<p>Regular, Medium and SemiBold for text and for the panel\'s interface. It covers Latin, Greek and Cyrillic, '
         f'with the rest of the Noto family for other scripts, and stays clear at small sizes on a 480-pixel screen.</p>'
         f'<p class="small">SIL Open Font License, from Google Fonts.</p></div></div>'
         f'<table class="tbl wide scale"><tr><th>Use</th><th>Typeface</th><th>Size</th><th>Example</th></tr>'
         f'<tr><td>Display</td><td>Space Grotesk 500</td><td>40 pt / 1.05</td><td class="g" style="font-size:20pt">Second life</td></tr>'
         f'<tr><td>Heading</td><td>Space Grotesk 500</td><td>24 pt / 1.1</td><td class="g" style="font-size:14pt">Install guide</td></tr>'
         f'<tr><td>Text</td><td>Noto Sans 400</td><td>10 pt / 1.55</td><td class="n">The panel connects by itself.</td></tr>'
         f'<tr><td>Label</td><td>Noto Sans 600</td><td>8 pt, +0.1 em, capitals</td><td class="n lab">DASHBOARD MODE</td></tr></table>')


def name_spelling():
    rows = [("PanelOS", True), ("Panel OS", False), ("PanelOs", False), ("Panelos", False), ("PANELOS", False), ("panelOS", False)]
    cells = "".join(f'<span class="{"ok" if ok else "no"}">{t}</span>' for t, ok in rows)
    page(f'<div class="kicker">Type and voice</div><h1>The name</h1><div class="cols2"><div>'
         f'<p class="lead">One word, capital P, capital O and S: <b>PanelOS</b>. Said “panel O S”.</p>'
         f'<div class="names">{cells}</div>'
         f'<ul class="bul"><li>In running text the name stands alone: “PanelOS starts in twelve seconds”, not “the PanelOS”. '
         f'With a noun it is fine: the PanelOS integration, a PanelOS panel.</li>'
         f'<li>In code, file names, the Home Assistant domain and URLs: <code>panelos</code>, all lower case.</li>'
         f'<li>The devices keep their own names: “an NSPanel Pro running PanelOS”. PanelOS is not a panel.</li></ul></div>'
         f'<div><div class="sub">Short forms</div><p>The mark stands for the P. The signature spells P O S. There is no other '
         f'abbreviation, and no nickname in public texts.</p><div class="sub">Next to other names</div><p>Home Assistant, '
         f'ESPHome and other projects are named as they write themselves, and their logos keep their own rules. PanelOS never '
         f'appears inside another logo, and no other logo is combined with the mark.</p></div></div>')


def voice():
    pairs = [
        ("The panel connects by itself.", "The device will now attempt to automatically establish a connection."),
        ("Home Assistant not reachable, retrying…", "Error 0x1F: network failure!"),
        ("Wake on approach works from about 45 cm.", "Revolutionary proximity technology for a seamless experience."),
    ]
    rows = "".join(f'<tr><td class="okc">{a}</td><td class="noc">{b}</td></tr>' for a, b in pairs)
    page(f'<div class="kicker">Type and voice</div><h1>How PanelOS speaks</h1><div class="cols2"><div>'
         f'<p class="lead">Like a good neighbour who knows electronics: plainly, briefly, and to the point.</p>'
         f'<ul class="bul"><li>Written for the people who install and use PanelOS. Technical details are welcome where they '
         f'help them.</li><li>Short sentences, active voice, the thing before the reason. One idea per sentence.</li>'
         f'<li>Facts, not promises: measured numbers with their conditions, “about” where it is about.</li>'
         f'<li>The panel\'s own screens may smile now and then; documents stay plain.</li></ul></div><div>'
         f'<table class="tbl wide voice"><tr><th>Like this</th><th>Not like this</th></tr>{rows}</table></div></div>')


def on_panel():
    SECTION["name"] = "In use"
    boot = boot_screen_svg("58mm")
    # the top bar of a screen: the settings button is the mark, not a gear
    bar = (f'<rect width="480" height="72" fill="{GRAPHITE}"/>'
           f'<text x="20" y="47" font-family="Space Grotesk" font-weight="500" font-size="32" fill="{PAPER}">12:48</text>'
           f'<circle cx="436" cy="36" r="24" fill="#26272B"/>'
           f'<g transform="translate({436 - 2.5 * 7.2:.1f} {36 - 1.5 * 7.2:.1f})">{mark(SIGNAL, PAPER, 7.2)}</g>')
    # a setup screen's head: the logo small at the top, Signal for the one action
    setup = (f'<rect width="480" height="300" fill="{NIGHT}"/>'
             f'<g transform="translate(160 34) scale(0.36)">{logo_inner(SIGNAL, PAPER, PAPER)[0]}</g>'
             f'<text x="40" y="128" font-family="Noto Sans" font-weight="500" font-size="22" fill="{PAPER}">Connect to Wi-Fi</text>'
             f'<rect x="40" y="150" width="400" height="48" rx="12" fill="#1C1D21"/>'
             f'<text x="58" y="181" font-family="Noto Sans" font-size="17" fill="#9A988F">Network name</text>'
             f'<rect x="40" y="220" width="400" height="48" rx="24" fill="{SIGNAL}"/>'
             f'<text x="240" y="250" text-anchor="middle" font-family="Noto Sans" font-weight="600" font-size="17" fill="#FFFFFF">Connect</text>')
    page(f'<div class="kicker">In use</div><h1>On the panel</h1><div class="cols2"><div>'
         f'<div class="figure">{boot}</div>'
         f'<ul class="bul" style="margin-top:6mm"><li><b>Start-up:</b> the stacked logo on Night with the quiet pattern, written in by '
         f'the start-up sequence. The same picture is the app\'s first frame, so nothing jumps.</li>'
         f'<li><b>Screensaver and dashboard:</b> no logo. The panel shows the home, not itself.</li></ul></div><div>'
         f'<div class="figure panelmini">{svg(setup, 480, 300, width="100%")}</div>'
         f'<ul class="bul" style="margin-top:4mm"><li><b>Setup and pairing</b> carry the PanelOS logo, small, at the top; no other '
         f'maker\'s logo appears on these screens. The accent is Signal, the grounds Night and Graphite, the text Paper: the '
         f'palette of these guidelines throughout.</li>'
         f'<li><b>Settings:</b> wherever the panel offers its settings, the button is the PanelOS mark, not a gear.</li></ul>'
         f'<div class="figure panelmini">{svg(bar, 480, 72, width="100%")}</div></div></div>')

def home_assistant():
    rowcard = (f'<div class="harow"><div class="haicon">{icon_svg("11mm")}</div><div><div class="hat">PanelOS</div>'
               f'<div class="has">1 device · Custom integration</div></div></div>')

    def device_header(dark):
        ground, card, text, sub = (("#111318", "#41475A", "#E6E6E6", "#A9AFBF") if dark else ("#F2F4F7", "#FFFFFF", "#212121", "#6B6A64"))
        inner, w, h = logo_inner(SIGNAL, PAPER if dark else INK, PAPER if dark else INK)
        k = 34 / h
        return svg(f'<rect width="600" height="150" fill="{ground}"/>'
                   f'<text x="24" y="34" font-family="Noto Sans" font-size="15" fill="#2E9BF0">In Kitchen</text>'
                   f'<g transform="translate({576 - w * k:.1f} 14) scale({k:.4f})">{inner}</g>'
                   f'<rect x="24" y="62" width="250" height="78" rx="8" fill="{card}"/>'
                   f'<text x="40" y="92" font-family="Noto Sans" font-size="17" fill="{text}">Device info</text>'
                   f'<text x="40" y="118" font-family="Noto Sans" font-size="12" fill="{sub}">NSPanel Pro · PanelOS 0.0.3</text>'
                   f'<rect x="290" y="62" width="286" height="78" rx="8" fill="{card}"/>'
                   f'<text x="306" y="92" font-family="Noto Sans" font-size="17" fill="{text}">Controls</text>', 600, 150, width="100%")

    files = [("icon.png", "256 × 256", "the badge, light and dark"), ("icon@2x.png", "512 × 512", "the badge, light and dark"),
             ("logo.png", "height 128", "logo for the light theme"), ("logo@2x.png", "height 256", "logo for the light theme"),
             ("dark_logo.png", "height 128", "logo for the dark theme"), ("dark_logo@2x.png", "height 256", "logo for the dark theme")]
    trs = "".join(f"<tr><td><code>{a}</code></td><td>{b}</td><td>{c}</td></tr>" for a, b, c in files)
    page(f'<div class="kicker">In use</div><h1>Home Assistant</h1><div class="cols2"><div>'
         f'<p class="lead">The integration ships its own images in <code>custom_components/panelos/brand</code>; Home '
         f'Assistant takes them from there for its lists and pages.</p>'
         f'<table class="tbl wide">{trs}</table>'
         f'<p class="small">Two kinds of image, two jobs. The <b>icon</b> is the badge: Ink with the mark, legible on light and dark '
         f'grounds alike, so one file serves both themes. The <b>logo</b> follows the theme: on the light theme the Ink wordmark, on '
         f'the dark theme the Paper wordmark with Paper dashes, as on the device and integration pages. All images have transparent '
         f'grounds.</p></div><div>'
         f'<div class="sub">Lists: the icon</div><div class="hapanel light">{rowcard}</div><div class="hapanel dark">{rowcard}</div>'
         f'<div class="sub" style="margin-top:5mm">Device page: the logo, top right</div>'
         f'<div class="figure framed">{device_header(False)}</div><div class="figure framed" style="margin-top:3mm">{device_header(True)}</div>'
         f'</div></div>')

def github_web():
    page(f'<div class="kicker">In use</div><h1>GitHub and the web</h1><div class="cols2"><div>'
         f'<div class="figure framed">{readme_header_svg("100%")}</div><p class="small">README header, 1280 × 320: Graphite '
         f'with the pattern, the logo in a calm panel, one line on what PanelOS is.</p>'
         f'<div class="figure framed">{readme_header_svg("100%", False)}</div><p class="small">The same on light, for '
         f'documentation pages.</p></div><div><div class="figure framed">{social_svg("100%")}</div>'
         f'<p class="small">Social preview, 1280 × 640, for the repository and shared links.</p>'
         f'<div class="avrow"><div class="avatar">{icon_svg("100%", GRAPHITE)}</div><p class="small">Avatar and favicon: the '
         f'app icon. Round crops keep the mark inside: it uses 62 % of the badge\'s width.</p></div></div></div>')


def files():
    kit = [
        ("mark/", "mark-color, mark-color-dark, mark-mono-ink, mark-mono-white (SVG)"),
        ("logo/", "logo, logo-dark, logo-stacked, logo-stacked-dark, logo-mono-ink, logo-mono-white (SVG, PNG at 1× and 2×)"),
        ("icon/", "app-icon (SVG; PNG at 16, 32, 48, 64, 128, 180, 256, 512, 1024)"),
        ("signature/", "signature, signature-dark (SVG)"),
        ("pattern/", "pattern-dark, pattern-light (SVG, 1600 × 900)"),
        ("social/", "readme-header (dark and light), social-preview (PNG)"),
        ("sound/", "start-up tone (WAV, 48 kHz)"),
        ("motion/", "the start-up as a 16:9 film with its tone, 3840 × 2160 and 1920 × 1080 (MP4, 60 fps), for presentations"),
    ]
    trs = "".join(f"<tr><td><code>{a}</code></td><td>{b}</td></tr>" for a, b in kit)
    page(f'<div class="kicker">In use</div><h1>Files and formats</h1><div class="cols2"><div>'
         f'<p class="lead">Everything is in the brand kit, PanelOS-Brand-Kit.zip, built from the same geometry as these pages.</p>'
         f'<table class="tbl wide">{trs}</table></div><div><ul class="bul"><li><b>SVG</b> for everything that can take it: '
         f'web, documents, print. The wordmark is outlined, so no font is needed.</li>'
         f'<li><b>PNG</b> where SVG is not accepted: Home Assistant, social networks, chat avatars.</li>'
         f'<li><b>Do not redraw</b> the mark from a screenshot; take the file. If a size is missing, scale the SVG.</li>'
         f'<li><b>Sources:</b> <code>branding/build_brand.py</code> builds every file, '
         f'<code>branding/guidelines/build_guidelines.py</code> these pages.</li></ul></div></div>')


def why():
    page(f'<div class="kicker">In use</div><h1>Why it looks like this</h1><div class="cols2"><div>'
         f'<p class="lead">A mark for open firmware should be honest about what it is. This one is a letter, written in a '
         f'code that was made to be read by anyone.</p><p>Pictures of screens, houses, waves and rings stand for many '
         f'smart home products. These four elements of Morse code belong to PanelOS: they spell its initial, they read the '
         f'same either way up, and they look like the screen it draws, with small and wide cards.</p></div><div>'
         f'<p>Red marks only what is lit, as on a panel at night. Everything else is ink and paper, so the identity is '
         f'calm in a home, clear at 16 pixels, and cheap to print in one colour.</p><p>The same code makes the pattern, the '
         f'start-up and its tone. One idea, used everywhere, is easier to remember than many.</p>'
         f'<div class="dashfig">{dash_figure()}</div></div></div>')


def dash_figure():
    """The mark beside a dashboard laid out the same way: dots are small cards, dashes wide ones."""
    u, k = 24, 2.6               # the mark's unit; the dashboard is the mark k times larger
    H = 3 * u * k
    m = mark(SIGNAL, INK, u, 0, (H - 3 * u) / 2)
    ox = 5 * u + 70
    cards = ""
    for kind, x, y, w, h in bb.mark_elements(u * k, ox, 0):
        cards += f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="12" fill="{PAPER}" stroke="{LINE}" stroke-width="1.5"/>'
        if kind == "dot":
            cards += f'<circle cx="{x + w / 2:.1f}" cy="{y + h / 2:.1f}" r="{h * 0.16:.1f}" fill="{SIGNAL}"/>'
        else:
            cards += (f'<rect x="{x + h * 0.28:.1f}" y="{y + h * 0.30:.1f}" width="{w * 0.5:.1f}" height="{h * 0.13:.1f}" rx="{h * 0.065:.1f}" fill="{INK}"/>'
                      f'<rect x="{x + h * 0.28:.1f}" y="{y + h * 0.56:.1f}" width="{w * 0.32:.1f}" height="{h * 0.1:.1f}" rx="{h * 0.05:.1f}" fill="#B9B7B0"/>')
    ay = H / 2
    arrow = (f'<path d="M{5 * u + 16} {ay} H{ox - 16} M{ox - 26} {ay - 9} L{ox - 15} {ay} L{ox - 26} {ay + 9}" fill="none" '
             f'stroke="#B9B7B0" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>')
    return svg(m + arrow + cards, ox + 5 * u * k, H, width="100%")


def back():
    bg = pattern_svg(1188, 840, 22, bb.PATTERN_DARK, NIGHT, phase=5)
    page(f'<div class="bgfill">{bg}</div><div class="bc"><div class="bc-mark">{mark_svg(SIGNAL, PAPER, "40mm")}</div>'
         f'<div class="bc-t">PanelOS</div><div class="bc-s">Open firmware for smart home wall panels. '
         f'Brand guidelines, version {VERSION}, {DATE}.</div></div>', "cover back", section="")


CSS = """
@page { size: 297mm 210mm; margin: 0 }
* { box-sizing: border-box }
html, body { margin: 0; background: #888 }
body { font-family: "Noto Sans", sans-serif; color: #101114; -webkit-print-color-adjust: exact; print-color-adjust: exact }
.page { width: 297mm; height: 210mm; position: relative; overflow: hidden; break-after: page; background: #fff; padding: 22mm 20mm 18mm; margin: 0 auto 6mm }
@media print { html, body { background: #fff } .page { margin: 0 } }
.hdr { position: absolute; top: 9mm; left: 20mm; right: 20mm; display: flex; justify-content: space-between; font-size: 6.8pt; letter-spacing: .12em; text-transform: uppercase; color: #8B8A84 }
.ftr { position: absolute; bottom: 8mm; left: 20mm; right: 20mm; display: flex; justify-content: space-between; font-size: 6.8pt; color: #8B8A84 }
h1 { font-family: "Space Grotesk"; font-weight: 500; font-size: 25pt; line-height: 1.1; margin: 0 0 7mm; letter-spacing: -.01em }
.kicker { font-family: "Space Grotesk"; font-size: 7.5pt; letter-spacing: .16em; text-transform: uppercase; color: #FF4A1C; font-weight: 500; margin-bottom: 2.5mm }
p { font-size: 9.6pt; line-height: 1.55; margin: 0 0 3mm; color: #2E2F33 }
p.lead { font-size: 12.5pt; line-height: 1.45; color: #101114 }
p.small, .small { font-size: 8pt; line-height: 1.5; color: #6B6A64 }
p.note { margin-top: 6mm; max-width: 220mm }
.center { text-align: center }
code { font-family: Consolas, "Cascadia Mono", monospace; font-size: .9em; background: #F1EFEA; padding: 0 .8mm; border-radius: .6mm }
.sub { font-family: "Space Grotesk"; font-size: 8pt; letter-spacing: .12em; text-transform: uppercase; color: #6B6A64; font-weight: 500; margin: 0 0 2.5mm }
ul.bul { margin: 0 0 3mm; padding-left: 4.5mm } ul.bul li { font-size: 9.6pt; line-height: 1.5; margin: 0 0 2.4mm; color: #2E2F33 }
.cols2 { display: grid; grid-template-columns: 1fr 1fr; gap: 14mm } .cols2.mid { align-items: center }
.grid4 { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6mm } .grid5 { display: grid; grid-template-columns: repeat(5, 1fr); gap: 5mm }
.grid5.forms { grid-template-columns: .8fr .75fr 1.5fr 1fr 1.15fr }
.figure { display: flex; justify-content: center; align-items: center } .figure.framed { border-radius: 2.5mm; overflow: hidden; line-height: 0 }
.stackfig { display: flex; flex-direction: column; gap: 6mm }
.bgfill { position: absolute; inset: 0; line-height: 0 } .bgfill svg { width: 100%; height: 100% }
.cover { padding: 0; background: #000 } .cv-calm { position: absolute; left: 14mm; top: 46mm; width: 190mm; height: 118mm; background: #000; border-radius: 6mm }
.cv-logo { position: absolute; left: 26mm; top: 62mm } .cv-title { position: absolute; left: 26mm; top: 104mm; font-family: "Space Grotesk"; font-weight: 500; font-size: 44pt; color: #F5F3EE; letter-spacing: -.01em }
.cv-sub { position: absolute; left: 26mm; top: 128mm; font-size: 10.5pt; line-height: 1.55; color: #B9B7B0 } .cv-ver { display: inline-block; margin-top: 4mm; color: #8B8A84; font-size: 8.5pt }
.divider { padding: 0; background: #000 } .dv { position: absolute; inset: 0 } .dv-num { position: absolute; left: 22mm; top: 18mm; font-family: "Space Grotesk"; font-size: 120pt; font-weight: 500; color: #F5F3EE; line-height: 1; background: #000; padding: 0 4mm 0 2mm; border-radius: 4mm }
.dv-box { position: absolute; left: 22mm; bottom: 22mm; background: #000; padding: 8mm 10mm 8mm 4mm; border-radius: 4mm; max-width: 200mm }
.dv-title { font-family: "Space Grotesk"; font-size: 36pt; font-weight: 500; color: #F5F3EE; margin-bottom: 3mm } .dv-note { color: #B9B7B0; font-size: 10.5pt; margin-bottom: 5mm }
.dv-list { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 2mm 7mm } .dv-list li { font-size: 9pt; color: #F5F3EE; opacity: .85 }
.dv-list li::before { content: ""; display: inline-block; width: 1.6mm; height: 1.6mm; border-radius: 50%; background: #FF4A1C; margin-right: 2mm; vertical-align: middle }
.toc { display: flex; flex-direction: column; gap: 4.2mm } .toc-row { display: grid; grid-template-columns: 12mm 46mm 1fr; align-items: baseline; border-bottom: 1px solid #ECEAE4; padding-bottom: 3mm }
.toc-n { font-family: "Space Grotesk"; color: #FF4A1C; font-weight: 500; font-size: 10pt } .toc-t { font-family: "Space Grotesk"; font-size: 13pt; font-weight: 500 } .toc-d { font-size: 8.6pt; color: #6B6A64 }
.codeline { display: flex; align-items: center; gap: 2.4mm; margin: 2mm 0 5mm } .codeline b { font-family: "Space Grotesk"; font-weight: 500; font-size: 14pt; margin-left: 4mm }
.dotc { width: 4mm; height: 4mm; border-radius: 50%; background: #FF4A1C } .dashc { width: 12mm; height: 4mm; border-radius: 2mm; background: #101114 }
.pcard { background: #F5F3EE; border-radius: 4mm; padding: 7mm; min-height: 90mm } .pnum { font-family: "Space Grotesk"; color: #FF4A1C; font-weight: 500; font-size: 9pt }
.ptitle { font-family: "Space Grotesk"; font-size: 15pt; font-weight: 500; margin: 2mm 0 3mm } .pcard p { font-size: 9.6pt; line-height: 1.55 }
.fcell, .vcell, .mcell { display: flex; flex-direction: column } .fart { height: 40mm; display: flex; align-items: center; justify-content: center; background: #F5F3EE; border-radius: 3mm; margin-bottom: 3mm }
.ft { font-family: "Space Grotesk"; font-size: 12pt; font-weight: 500; margin-bottom: 1.5mm }
.vart { height: 46mm; display: flex; align-items: center; justify-content: center; border-radius: 3mm; margin-bottom: 3mm; border: 1px solid #ECEAE4 }
.mgrid { row-gap: 7mm } .mart { position: relative; height: 34mm; display: flex; align-items: center; justify-content: center; background: #F5F3EE; border-radius: 3mm; margin-bottom: 2mm; overflow: hidden }
.xmark { position: absolute; right: 3mm; top: 3mm; width: 6mm; height: 6mm; border-radius: 50%; background: #FF4A1C } .xmark::before, .xmark::after { content: ""; position: absolute; left: 1.4mm; top: 2.75mm; width: 3.2mm; height: .5mm; background: #fff; transform: rotate(45deg) } .xmark::after { transform: rotate(-45deg) }
.fake { font-family: Georgia, serif; font-size: 15pt; font-style: italic; margin-left: 2mm }
.szrow { display: flex; align-items: flex-end; gap: 8mm; margin-bottom: 7mm } .szc { display: flex; flex-direction: column; align-items: center; gap: 2mm; font-size: 7.5pt; color: #6B6A64 }
table.tbl { border-collapse: collapse; font-size: 8.8pt; margin: 0 0 4mm } table.tbl td, table.tbl th { text-align: left; padding: 2mm 5mm 2mm 0; border-bottom: 1px solid #ECEAE4; vertical-align: top }
table.tbl th { font-family: "Space Grotesk"; font-weight: 500; font-size: 8pt; letter-spacing: .06em; color: #6B6A64; text-transform: uppercase } table.tbl.wide { width: 100% }
.sw .swc { height: 34mm; border-radius: 3mm; display: flex; align-items: flex-end; padding: 3mm; font-family: "Space Grotesk"; font-weight: 500; font-size: 12pt; border: 1px solid #ECEAE4 }
.swv { font-size: 8pt; line-height: 1.55; color: #2E2F33; margin: 2.5mm 0 1.5mm } .sw.sm .swc { height: 12mm } .pgrid .swv { margin-top: 1.5mm }
.chip { display: inline-block; padding: 1mm 3mm; border-radius: 1.5mm; font-family: "Space Grotesk"; font-weight: 500; border: 1px solid #ECEAE4 }
.pbar { display: flex; height: 12mm; border-radius: 2mm; overflow: hidden; margin: 2mm 0 1.5mm } .pbar-l { display: flex; justify-content: space-between; font-size: 7.5pt; color: #6B6A64; margin-bottom: 5mm }
.panelframe { border-radius: 7mm; overflow: hidden; box-shadow: 0 0 0 3mm #2A2B2F; line-height: 0 }
.kfrow { display: flex; gap: 4mm; margin: 2mm 0 6mm } .kf { display: flex; flex-direction: column; align-items: center; gap: 1.5mm; font-size: 7.5pt; color: #6B6A64 } .kf svg { border-radius: 2mm }
.timeline { position: relative; padding-left: 26mm; margin-top: 2mm } .ticks { position: relative; height: 5mm; margin-left: 0; border-bottom: 1px solid #DCD9D0 }
.tick { position: absolute; font-size: 6.8pt; color: #8B8A84; transform: translateX(-50%) }
.lane { display: flex; align-items: center; height: 6mm } .ln { position: absolute; left: 0; width: 24mm; font-size: 8pt; color: #2E2F33 }
.lt { position: relative; flex: 1; height: 6mm } .lb { position: absolute; top: 1.6mm; height: 2.8mm; border-radius: 1.4mm } .lnote { position: absolute; top: .9mm; font-size: 7.2pt; color: #6B6A64; white-space: nowrap }
.srow { position: relative; height: 16mm; margin: 2mm 0 6mm } .sblk { position: absolute; top: 0; height: 8mm; background: #101114; border-radius: 1mm } .slbl { position: absolute; top: 9.5mm; font-size: 7pt; color: #6B6A64; text-align: center }
.tface .tname { font-size: 10pt; color: #6B6A64; margin-bottom: 1mm } .tsample { font-size: 44pt; line-height: 1.1; margin-bottom: 4mm }
.g { font-family: "Space Grotesk"; font-weight: 500 } .n { font-family: "Noto Sans" } .lab { font-weight: 600; font-size: 8pt; letter-spacing: .1em }
table.scale { margin-top: 7mm } table.scale td { vertical-align: middle }
.names { display: flex; flex-wrap: wrap; gap: 3mm; margin: 2mm 0 6mm } .names span { font-family: "Space Grotesk"; font-weight: 500; font-size: 13pt; padding: 1.5mm 4mm; border-radius: 2mm }
.names .ok { background: #101114; color: #F5F3EE } .names .no { background: #F5F3EE; color: #9A988F; text-decoration: line-through }
table.voice td { font-size: 9pt; padding-right: 6mm } .okc::before { content: "● "; color: #FF4A1C } .noc { color: #8B8A84 }
.hapanel { border-radius: 3mm; padding: 5mm; margin-bottom: 4mm } .hapanel.light { background: #F2F4F7 } .hapanel.dark { background: #1C1C1E }
.harow { display: flex; align-items: center; gap: 4mm; background: #fff; border-radius: 2.5mm; padding: 3mm 4mm } .dark .harow { background: #2B2B2E }
.hat { font-size: 10pt; font-weight: 500 } .has { font-size: 8pt; color: #6B6A64 } .dark .hat { color: #F5F3EE } .dark .has { color: #9A988F }
.avrow { display: flex; align-items: center; gap: 6mm; margin-top: 4mm } .avatar { width: 24mm; height: 24mm; border-radius: 50%; overflow: hidden; flex: none; line-height: 0 } .avatar svg { width: 30mm; height: 30mm; margin: -3mm }
.dashfig { margin-top: 8mm }
.mc-ok { display: flex; align-items: center; gap: 6mm; background: #F5F3EE; border-radius: 3mm; padding: 4mm 6mm; margin: -2mm 0 6mm; font-size: 9.2pt; color: #2E2F33 }
.mc-okart { display: flex; flex: none } .mgrid2 { row-gap: 6mm }
.mc { display: flex; flex-direction: column } .mc-art { position: relative; height: 38mm; display: flex; align-items: center; justify-content: center; background: #FAF9F6; border: 1px solid #ECEAE4; border-radius: 3mm; padding: 4mm 6mm; margin-bottom: 2.5mm }
.mc-art svg { max-height: 30mm }
.mc-x { position: absolute; left: 3mm; top: 3mm; width: 5.5mm; height: 5.5mm; border-radius: 50%; background: #FF4A1C }
.mc-x::before, .mc-x::after { content: ""; position: absolute; left: 1.35mm; top: 2.5mm; width: 2.8mm; height: .5mm; border-radius: .25mm; background: #fff; transform: rotate(45deg) } .mc-x::after { transform: rotate(-45deg) }
.mc-t { font-family: "Space Grotesk"; font-size: 11pt; font-weight: 500; margin-bottom: 1mm }
.panelmini { border-radius: 3mm; overflow: hidden; line-height: 0 } .back .bc { position: absolute; left: 22mm; bottom: 24mm; background: #000; padding: 8mm 10mm 8mm 4mm; border-radius: 4mm } .bc-t { font-family: "Space Grotesk"; font-size: 24pt; font-weight: 500; color: #F5F3EE; margin-top: 6mm } .bc-s { font-size: 9pt; color: #B9B7B0; max-width: 150mm; margin-top: 2mm }
svg text.dim, .dim { font-family: "Space Grotesk"; font-size: 13px; fill: #6B6A64 }
"""


def build():
    cover()
    contents()
    divider("01", "The idea", "What the mark is, and what it stands for.", ["A letter in light", "Four principles"])
    idea()
    principles()
    divider("02", "The mark", "How it is built and how it is used.",
            ["Built on one unit", "Five forms", "Space and size", "Colour versions", "What not to do"])
    construction()
    forms()
    space_size()
    colour_versions()
    misuse()
    divider("03", "Colour", "Mostly dark and light, a little red.", ["The palette", "Proportion and contrast"])
    palette()
    proportion()
    divider("04", "The pattern", "The code as texture.", ["The code as texture", "Where it goes"])
    pattern_build()
    pattern_use()
    divider("05", "Motion and sound", "The start-up writes the mark in Morse.", ["The start-up", "The tone"])
    motion()
    sound()
    divider("06", "Type and voice", "Two typefaces, one name, a plain voice.", ["Two typefaces", "The name", "How PanelOS speaks"])
    typefaces()
    name_spelling()
    voice()
    divider("07", "In use", "Where PanelOS appears.",
            ["On the panel", "Home Assistant", "GitHub and the web", "Files and formats", "Why it looks like this"])
    on_panel()
    home_assistant()
    github_web()
    files()
    why()
    back()
    html = (f'<!doctype html><html lang="en"><head><meta charset="utf-8"><title>PanelOS brand guidelines</title>'
            f'{bb.FONT_LINK}<style>{CSS}</style></head><body>{"".join(PAGES)}</body></html>')
    out = HERE / "guidelines.html"
    out.write_text(html, encoding="utf-8", newline="\n")
    print(f"{len(PAGES)} pages -> {out}")
    return out


def pdf(html_path: Path):
    edge = bb.find_edge()
    target = BRANDING / "PanelOS-Brand-Guidelines.pdf"
    subprocess.run([edge, "--headless=new", "--disable-gpu", "--no-pdf-header-footer", "--virtual-time-budget=15000",
                    f"--print-to-pdf={target}", html_path.as_uri()], capture_output=True, timeout=180)
    print("pdf ->", target, target.stat().st_size if target.exists() else "missing")


if __name__ == "__main__":
    path = build()
    if "--no-pdf" not in sys.argv:
        pdf(path)
