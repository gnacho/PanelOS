"""Minimal, exact SVG path rasterizer for the boot splash (no Cairo needed).

Supports the path commands CorelDRAW writes (M m L l H h V v C c Z z) and
both fill rules; anti-aliasing by supersampling. Coverage maps are numpy
float arrays (0..1).
"""

import re

import numpy as np

CURVE_STEPS = 32


def parse_paths(svg_text):
    """[(class, [subpath points]), ...] for every <path> in the file."""
    out = []
    for m in re.finditer(r"<path\b([^>]*)>", svg_text):
        attrs = m.group(1)
        d = re.search(r'\bd="([^"]+)"', attrs)
        cls = re.search(r'\bclass="([^"]+)"', attrs)
        if d:
            out.append((cls.group(1) if cls else "", _subpaths(d.group(1))))
    return out


def _subpaths(d):
    toks = re.findall(r"[MmLlHhVvCcZz]|-?(?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][-+]?[0-9]+)?", d)
    i, cmd = 0, None
    cur = start = (0.0, 0.0)
    subs, pts = [], []

    def num():
        nonlocal i
        v = float(toks[i])
        i += 1
        return v

    while i < len(toks):
        if toks[i].isalpha():
            cmd = toks[i]
            i += 1
            if cmd in "Zz":
                if pts:
                    subs.append(pts)
                pts, cur = [], start
                continue
        if cmd in "Mm":
            x, y = num(), num()
            cur = (cur[0] + x, cur[1] + y) if cmd == "m" else (x, y)
            if pts:
                subs.append(pts)
            start, pts = cur, [cur]
            cmd = "l" if cmd == "m" else "L"
        elif cmd in "Ll":
            x, y = num(), num()
            cur = (cur[0] + x, cur[1] + y) if cmd == "l" else (x, y)
            pts.append(cur)
        elif cmd in "Hh":
            x = num()
            cur = (cur[0] + x if cmd == "h" else x, cur[1])
            pts.append(cur)
        elif cmd in "Vv":
            y = num()
            cur = (cur[0], cur[1] + y if cmd == "v" else y)
            pts.append(cur)
        elif cmd in "Cc":
            c = [num() for _ in range(6)]
            if cmd == "c":
                c = [cur[0] + c[0], cur[1] + c[1], cur[0] + c[2], cur[1] + c[3], cur[0] + c[4], cur[1] + c[5]]
            p0 = cur
            for k in range(1, CURVE_STEPS + 1):
                t = k / CURVE_STEPS
                mt = 1 - t
                pts.append((mt**3 * p0[0] + 3 * mt * mt * t * c[0] + 3 * mt * t * t * c[2] + t**3 * c[4],
                            mt**3 * p0[1] + 3 * mt * mt * t * c[1] + 3 * mt * t * t * c[3] + t**3 * c[5]))
            cur = (c[4], c[5])
        else:
            raise ValueError(f"unsupported path command {cmd}")
    if pts:
        subs.append(pts)
    return subs


def bounds(subpaths):
    xs = [x for s in subpaths for x, _ in s]
    ys = [y for s in subpaths for _, y in s]
    return min(xs), min(ys), max(xs), max(ys)


def coverage(subpaths, width, height, scale, ox, oy, rule="nonzero", ss=8):
    """Coverage map (height x width) of the filled subpaths, drawn at
    x' = (x - ox) * scale, y' = (y - oy) * scale."""
    W, H = width * ss, height * ss
    edges = []
    for s in subpaths:
        p = np.array(s, np.float64)
        p = (p - [ox, oy]) * scale * ss
        q = np.roll(p, -1, axis=0)
        edges.append(np.hstack([p, q]))
    e = np.vstack(edges)
    x0, y0, x1, y1 = e[:, 0], e[:, 1], e[:, 2], e[:, 3]
    keep = y0 != y1
    x0, y0, x1, y1 = x0[keep], y0[keep], x1[keep], y1[keep]
    wind = np.where(y1 > y0, 1, -1)
    ylo, yhi = np.minimum(y0, y1), np.maximum(y0, y1)
    big = np.zeros((H, W), np.uint8)
    xs_grid = np.arange(W) + 0.5
    for row in range(H):
        y = row + 0.5
        act = (ylo <= y) & (y < yhi)
        if not act.any():
            continue
        t = (y - y0[act]) / (y1[act] - y0[act])
        xc = x0[act] + t * (x1[act] - x0[act])
        w = wind[act]
        order = np.argsort(xc)
        xc, w = xc[order], w[order]
        # winding number left of each sample = sum of crossings with x < sample
        idx = np.searchsorted(xc, xs_grid)
        csum = np.concatenate([[0], np.cumsum(w)])
        wn = csum[idx]
        if rule == "evenodd":
            cnt = np.concatenate([[0], np.arange(1, len(xc) + 1)])[idx]
            inside = (cnt % 2) == 1
        else:
            inside = wn != 0
        big[row] = inside
    return big.reshape(height, ss, width, ss).mean(axis=(1, 3)).astype(np.float32)
