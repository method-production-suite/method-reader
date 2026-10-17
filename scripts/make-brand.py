#!/usr/bin/env python3
"""Draw Method Reader's mark and wordmark, and check the site's brand colors. Standard
library only.

    python3 scripts/make-brand.py             write the files below
    python3 scripts/make-brand.py --check     change nothing; exit 1 if a file differs or
                                              a color pair in tokens.css fails its contrast
    python3 scripts/make-brand.py --contrast  print the contrast table for docs/BRAND.md

Writes, under site/:

    favicon.svg                    the small mark on a 16-unit grid, light and dark
    favicon.ico                    the same at 16 and 32 pixels, for browsers that want one
    apple-touch-icon.png           180 pixels, the white mark on violet, for phone home screens
    assets/brand/mark.svg          the mark, for light backgrounds
    assets/brand/mark-dark.svg     the mark, for dark backgrounds
    assets/brand/wordmark.svg      mark and name, for light backgrounds
    assets/brand/wordmark-dark.svg mark and name, for dark backgrounds

The mark is an e-reader showing a script page: the device body at full strength, and
on its screen the shape a screenplay keeps when it reflows (a scene heading, action, a
cue and its dialogue) at half strength. The screen is empty, so the mark sits on any
background. It is the family's box (40 by 52 units, Method Budget's page and Method
Schedule's stripboard). The name is drawn from strokes on a 100-unit x-height
(lowercase, geometric, one weight), not set in a font, so it looks the same everywhere
and ships no font file. m, e, t, h, o and d are Method Budget's letters; r and a are new
here and follow the same rules.

This file is a copy of Method Schedule's scripts/make-brand.py with our mark, our
letters and our accent; keep the shared parts in step with it. The window keeps
Screepub's own theme (brand/tokens.json) until the rename release moves it to these
colors. Rules for using all of this are in docs/BRAND.md.
"""

from __future__ import annotations

import math
import re
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "site" / "assets" / "brand"
TOKENS = ROOT / "site" / "assets" / "tokens.css"

# Brand colors (site/assets/tokens.css): our violet, and the ink of each scheme.
ACCENT = "#7052BA"
ACCENT_DARK = "#B49CF0"
INK = "#1C2228"
INK_DARK = "#E4E8EC"
NAME = "Method Reader"


def fmt(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


# ----------------------------------------------------------------------------- mark
# 64-unit grid, y down. The device runs x 12..52, y 6..58, the family's box, with a
# 4-unit bezel and a 10-unit chin under the screen (where a reader has its buttons).
# On the screen, the script's shape: a scene heading across the full measure, two lines
# of action (the second ragged), the cue set in, and two lines of dialogue between the
# cue's margin and the action's. Lines are 3 units tall with 3-unit gaps.
# A shape is (x, y, w, h, r, strength): strength 1 is full color, .5 half. The device is
# drawn as its body with the screen cut out (`cut`), so the screen shows the background.

BODY = (12, 6, 40, 52, 5)
SCREEN = (16, 10, 32, 38, 1.5)
LINES = [
    (19, 14, 26, 3),  # scene heading
    (19, 20, 26, 3),  # action
    (19, 26, 18, 3),  # action, ragged
    (28, 32, 10, 3),  # cue
    (23, 38, 18, 3),  # dialogue
    (23, 44, 12, 3),  # dialogue, last line
]


def mark_rows() -> list[tuple[float, float, float, float, float, float]]:
    return [BODY + (1.0,)] + [l + (1.5, 0.5) for l in LINES]


# 16-unit favicon grid: whole units so it stays sharp at 16 and 32 pixels. The device
# with a 1-unit bezel and a 3-unit chin, and three 1-unit lines on the screen.
FAV_BODY = (3, 1, 10, 14, 1.5)
FAV_SCREEN = (4, 2, 8, 9, 0.5)
FAV_LINES = [(5, 3, 6, 1), (5, 5, 4, 1), (6, 7, 5, 1), (6, 9, 3, 1)]


def fav_rows() -> list[tuple[float, float, float, float, float, float]]:
    return [FAV_BODY + (1.0,)] + [l + (0.5, 0.5) for l in FAV_LINES]


def cut_of(rows) -> tuple[float, float, float, float, float]:
    return SCREEN if rows[0][:5] == BODY else FAV_SCREEN


def rrect(x: float, y: float, w: float, h: float, r: float) -> str:
    """A rounded rectangle as a closed subpath."""
    return (
        f"M{fmt(x + r)} {fmt(y)}H{fmt(x + w - r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)} {fmt(r)}"
        f"V{fmt(y + h - r)}a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)} {fmt(r)}H{fmt(x + r)}"
        f"a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)}-{fmt(r)}V{fmt(y + r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)}-{fmt(r)}Z"
    )


def rows_paths(rows, color: str | None) -> str:
    """Two paths: the script's lines at half strength, then the device at full with its
    screen cut out (even-odd, so the screen shows whatever is behind the mark)."""
    fill = f' fill="{color}"' if color else ""
    half = "".join(rrect(*r[:5]) for r in rows if r[5] < 1)
    full = "".join(rrect(*r[:5]) for r in rows if r[5] == 1) + rrect(*cut_of(rows))
    return (f'<path{fill} opacity=".5" d="{half}"/>'
            f'<path{fill} fill-rule="evenodd" d="{full}"/>')


# ----------------------------------------------------------------------------- name
# Strokes on a y-up grid: baseline 0, x-height 100, ascender 140, stroke 18. Round
# letters overshoot the baseline and x-height by 1.5 so they look as tall as the stems.
# m, e, t, h, o, d and u are Method Budget's letters unchanged; s, c and l are Method
# Schedule's; r and a are new here and follow the same rules.

W = 18.0
R = 43.5  # bowl radius at the stroke's center
C = 50.0  # bowl center height
ASC = 140.0
TASC = 126.0  # t is shorter than d, h and l
BOWL = 2 * R + W  # 105: width of a bowl letter


def bowl(x0: float) -> list[str]:
    cx = x0 + W / 2 + R
    return [f"M{cx - R} {C}A{R} {R} 0 1 1 {cx + R} {C}A{R} {R} 0 1 1 {cx - R} {C}Z"]


def on_bowl(cx: float, cy: float, rx: float, ry: float, deg: float) -> tuple[float, float]:
    a = math.radians(deg)
    return cx + rx * math.cos(a), cy + ry * math.sin(a)


def g_m(x0):
    r = 34.0
    c = 100 - W / 2 - r + 1.5  # arch tops overshoot like the bowls
    s1, s2, s3 = x0 + W / 2, x0 + W / 2 + 2 * r, x0 + W / 2 + 4 * r
    return [
        f"M{s1} 0V100",
        f"M{s1} {c}A{r} {r} 0 0 1 {s2} {c}V0",
        f"M{s2} {c}A{r} {r} 0 0 1 {s3} {c}V0",
    ], 4 * r + W


def g_e(x0):
    cx = x0 + W / 2 + R
    ex, ey = on_bowl(cx, C, R, R, -38)
    return [f"M{cx - R} {C}H{cx + R}A{R} {R} 0 1 0 {ex:.2f} {ey:.2f}"], BOWL


def g_c(x0):
    """An o opened on the right, its ends cut where the e's tail ends."""
    cx = x0 + W / 2 + R
    sx, sy = on_bowl(cx, C, R, R, 40)
    ex, ey = on_bowl(cx, C, R, R, -40)
    return [f"M{sx:.2f} {sy:.2f}A{R} {R} 0 1 0 {ex:.2f} {ey:.2f}"], BOWL - 10


def g_s(x0):
    """Two stacked loops on one spine: over the top and down the left, across the
    middle, down the right and back along the bottom."""
    rx = 31.0
    ry = (100 + 1.5 - W / 2 - (-1.5 + W / 2)) / 4  # 21.25: the two loops share the height
    cx = x0 + W / 2 + rx
    up, lo = 100 + 1.5 - W / 2 - ry, -1.5 + W / 2 + ry
    sx, sy = on_bowl(cx, up, rx, ry, 28)
    ex, ey = on_bowl(cx, lo, rx, ry, -152)
    return [
        f"M{sx:.2f} {sy:.2f}A{rx} {ry} 0 1 0 {cx} {C}"
        f"A{rx} {ry} 0 1 1 {ex:.2f} {ey:.2f}"
    ], 2 * rx + W


def g_t(x0):
    stem = x0 + 22
    return [f"M{stem} {TASC}V0", f"M{x0} {100 - W / 2}H{x0 + 54}"], 54.0


def g_h(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 0V{ASC}", f"M{l} {C}A{R} {R} 0 0 1 {r} {C}V0"], BOWL


def g_l(x0):
    return [f"M{x0 + W / 2} 0V{ASC}"], W


def g_o(x0):
    return bowl(x0), BOWL


def g_d(x0):
    return bowl(x0) + [f"M{x0 + W / 2 + 2 * R} 0V{ASC}"], BOWL


def g_u(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 100V{C}A{R} {R} 0 0 0 {r} {C}", f"M{r} 100V0"], BOWL


def g_a(x0):
    """A single-storey a: the bowl with a stem on its right that stops at the x-height,
    the d without its ascender."""
    return bowl(x0) + [f"M{x0 + W / 2 + 2 * R} 0V{100 + 1.5 - W / 2}"], BOWL


def g_r(x0):
    """A stem and the h's shoulder, stopped where it starts to fall."""
    l = x0 + W / 2
    ex, ey = on_bowl(l + R, C, R, R, 35)
    return [f"M{l} 0V100", f"M{l} {C}A{R} {R} 0 0 1 {ex:.2f} {ey:.2f}"], ex - x0 + W / 2


GLYPHS = {"m": g_m, "e": g_e, "t": g_t, "h": g_h, "o": g_o, "d": g_d, "u": g_u,
          "s": g_s, "c": g_c, "l": g_l, "a": g_a, "r": g_r}
# Space between neighbours, outer edge to outer edge: tighter beside a bowl. The pairs
# Method Budget's and Method Schedule's names have keep their values.
GAP = {"me": 13, "et": 9, "th": 14, "ho": 13, "od": 12, "ud": 13,
       "sc": 11, "ch": 13, "he": 13, "ed": 12, "du": 14, "ul": 16, "le": 13,
       "re": 6, "ea": 12, "ad": 12, "de": 12, "er": 13}
WORD_SPACE = 52.0


def set_name(text: str) -> tuple[list[str], float]:
    """Paths (y up) and the advance width for a lowercase phrase."""
    x, paths, prev = 0.0, [], ""
    for ch in text:
        if ch == " ":
            x += WORD_SPACE
            prev = ""
            continue
        if prev:
            x += GAP.get(prev + ch, 13)
        p, w = GLYPHS[ch](x)
        paths += p
        x += w
        prev = ch
    return paths, x


def flip(path: str, base: float) -> str:
    """Turn a y-up path into SVG's y-down space with the baseline at `base`. Arc sweep
    flags are written as they look on screen (1 turns clockwise), so they stay."""
    out, tokens = [], tokenize(path)
    i = 0
    cmd = ""
    while i < len(tokens):
        t = tokens[i]
        if t.isalpha():
            cmd = t
            out.append(t)
            i += 1
            continue
        if cmd in "ML":
            x, y = float(tokens[i]), float(tokens[i + 1])
            out.append(f"{fmt(x)} {fmt(base - y)}")
            i += 2
        elif cmd == "H":
            out.append(fmt(float(t)))
            i += 1
        elif cmd == "V":
            out.append(fmt(base - float(t)))
            i += 1
        elif cmd == "A":
            rx, ry, rot, large, sweep, x, y = tokens[i : i + 7]
            out.append(
                f"{fmt(float(rx))} {fmt(float(ry))} {rot} {large} {sweep} "
                f"{fmt(float(x))} {fmt(base - float(y))}"
            )
            i += 7
        else:
            raise ValueError(f"unsupported command {cmd!r}")
    return " ".join(out).replace(" Z", "Z")


def tokenize(path: str) -> list[str]:
    toks, num = [], ""
    for ch in path:
        if ch.isalpha():
            if num:
                toks.append(num)
                num = ""
            toks.append(ch)
        elif ch in " ,":
            if num:
                toks.append(num)
                num = ""
        elif ch == "-" and num and num[-1] not in "eE":
            toks.append(num)
            num = ch
        else:
            num += ch
    if num:
        toks.append(num)
    return toks


# ----------------------------------------------------------------------------- pixels
# The two raster files browsers still ask for, drawn here with 8x8 supersampling so no
# image tool is needed: favicon.ico (16 and 32 px, from the 16-unit favicon rows) and
# apple-touch-icon.png (180 px, the white mark on a full-bleed violet square; iOS
# rounds the corners itself).


def in_rrect(u, v, x, y, w, h, r):
    if not (x <= u <= x + w and y <= v <= y + h):
        return False
    cx = min(max(u, x + r), x + w - r)
    cy = min(max(v, y + r), y + h - r)
    return (u - cx) ** 2 + (v - cy) ** 2 <= r * r


def cover_of(rows):
    cut = cut_of(rows)

    def cover(u, v):
        for x, y, w, h, r, a in rows:
            if a == 1.0 and in_rrect(u, v, *cut):
                continue  # the screen is cut out of the body
            if in_rrect(u, v, x, y, w, h, r):
                return a
        return 0.0
    return cover


def hex_rgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def raster(size, units, cover, fg, bg=None, inset=0.0):
    """RGBA rows. `units` is the drawing's grid size, shown across size minus inset on
    each side. bg None means transparent."""
    ss = 8
    scale = units / (size - 2 * inset)
    fr, fgc, fb = hex_rgb(fg)
    out = bytearray()
    for j in range(size):
        out.append(0)
        for i in range(size):
            acc = 0.0
            for sy in range(ss):
                for sx in range(ss):
                    u = (i - inset + (sx + 0.5) / ss) * scale
                    v = (j - inset + (sy + 0.5) / ss) * scale
                    acc += cover(u, v)
            a = acc / (ss * ss)
            if bg is None:
                out += bytes((fr, fgc, fb, round(255 * a)))
            else:
                br, bgc, bb = hex_rgb(bg)
                out += bytes((round(br + (fr - br) * a), round(bgc + (fgc - bgc) * a),
                              round(bb + (fb - bb) * a), 255))
    return bytes(out)


def png(size, rows):
    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def ico(images):
    """An .ico holding PNG images: [(size, png bytes)]."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, data = b"", b""
    for size, blob in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset + len(data))
        data += blob
    return head + entries + data


def favicon_ico() -> bytes:
    return ico([(n, png(n, raster(n, 16, cover_of(fav_rows()), ACCENT))) for n in (16, 32)])


def touch_icon() -> bytes:
    return png(180, raster(180, 64, cover_of(mark_rows()), "#FFFFFF", bg=ACCENT, inset=14))


# ----------------------------------------------------------------------------- files

HEADER = "<!-- Method Reader {what}. Drawn by scripts/make-brand.py; edit that, not this file. -->"


def mark_svg(color: str, label: str) -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img"'
        f' aria-label="{label}">\n{HEADER.format(what="mark")}\n'
        f"<title>{label}</title>\n{rows_paths(mark_rows(), color)}\n</svg>\n"
    )


def wordmark_svg(mark: str, ink: str) -> str:
    """The mark, as tall as the ascenders, then the name, on one baseline. Drawn in the
    name's own units: baseline at y 150, ascenders reach y 10."""
    paths, width = set_name("method reader")
    base = 150.0
    d = " ".join(flip(p, base) for p in paths)
    k = (base - 10) / 52  # the board (52 units tall) spans ascender to baseline
    tx, ty = -12 * k, 10 - 6 * k
    name_x = 40 * k + 40  # board width, then a gap
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(name_x + width)} 162"'
        f' role="img" aria-label="{NAME}">\n'
        f"{HEADER.format(what='wordmark')}\n<title>{NAME}</title>\n"
        f'<g transform="translate({fmt(tx)} {fmt(ty)}) scale({k:.4f})">{rows_paths(mark_rows(), mark)}</g>\n'
        f'<path transform="translate({fmt(name_x)} 0)" fill="none" stroke="{ink}"'
        f' stroke-width="{fmt(W)}" d="{d}"/>\n</svg>\n'
    )


def favicon_svg() -> str:
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">\n'
        f"{HEADER.format(what='favicon')}\n"
        f"<style>path{{fill:{ACCENT}}}@media (prefers-color-scheme:dark){{path{{fill:{ACCENT_DARK}}}}}</style>\n"
        f"{rows_paths(fav_rows(), None)}\n</svg>\n"
    )


def outputs() -> dict[Path, str | bytes]:
    return {
        ROOT / "site" / "favicon.ico": favicon_ico(),
        ROOT / "site" / "apple-touch-icon.png": touch_icon(),
        ROOT / "site" / "favicon.svg": favicon_svg(),
        BRAND / "mark.svg": mark_svg(ACCENT, NAME),
        BRAND / "mark-dark.svg": mark_svg(ACCENT_DARK, NAME),
        BRAND / "wordmark.svg": wordmark_svg(ACCENT, INK),
        BRAND / "wordmark-dark.svg": wordmark_svg(ACCENT_DARK, INK_DARK),
    }


def canonical(data: bytes) -> bytes:
    """What a file means rather than its exact bytes: a PNG's pixels (zlib versions
    compress differently, so CI on Linux and a Mac never write the same bytes), each
    image of an .ico likewise, anything else as is."""
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        pos, idat, head = 8, b"", b""
        while pos < len(data):
            n, kind = struct.unpack(">I4s", data[pos : pos + 8])
            body = data[pos + 8 : pos + 8 + n]
            if kind == b"IHDR":
                head = body
            elif kind == b"IDAT":
                idat += body
            pos += 12 + n
        return head + zlib.decompress(idat)
    if data[:4] == b"\x00\x00\x01\x00":
        count = struct.unpack("<H", data[4:6])[0]
        out = b""
        for k in range(count):
            size, off = struct.unpack("<II", data[6 + 16 * k + 8 : 6 + 16 * k + 16])
            out += canonical(data[off : off + size])
        return out
    return data


# ----------------------------------------------------------------------------- contrast
# The pairs docs/BRAND.md lists: Method Schedule's table without its stripboard pairs,
# plus the reader drawing's own (its screen text, and the muted ink on that screen).
# WCAG 2.x: body text 4.5, borders that carry meaning 3.

PAIRS = [
    ("ink", "bg", 4.5), ("ink", "surface-2", 4.5), ("ink-soft", "surface-2", 4.5),
    ("muted", "bg", 4.5), ("muted", "surface-2", 4.5),
    ("accent", "bg", 4.5), ("accent", "surface", 4.5), ("accent", "surface-2", 4.5),
    ("accent", "accent-soft", 4.5), ("accent-strong", "accent-soft", 4.5),
    ("accent-ink", "accent", 4.5), ("ink", "accent-soft", 4.5),
    ("note", "surface", 4.5), ("rule-strong", "bg", 3.0),
    ("screen-ink", "screen", 4.5), ("screen-muted", "screen", 4.5),
]


def read_tokens() -> tuple[dict[str, str], dict[str, str]]:
    """Hex colors of the light :root block and the dark override in tokens.css."""
    text = re.sub(r"/\*.*?\*/", "", TOKENS.read_text(encoding="utf-8"), flags=re.S)
    m = re.search(r"@media\s*\(prefers-color-scheme:\s*dark\)", text)
    light_part, dark_part = (text[: m.start()], text[m.end():]) if m else (text, "")
    rx = re.compile(r"--([a-z0-9-]+)\s*:\s*(#[0-9A-Fa-f]{6})\b")
    light = dict(rx.findall(light_part))
    dark = dict(light, **dict(rx.findall(dark_part)))
    return light, dark


def luminance(h: str) -> float:
    c = [int(h[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def ratio(a: str, b: str) -> float:
    x, y = sorted((luminance(a), luminance(b)), reverse=True)
    return (x + 0.05) / (y + 0.05)


def contrast_rows() -> list[tuple[str, float, float, float]]:
    light, dark = read_tokens()
    rows = []
    for fg, bg, need in PAIRS:
        for scheme in (light, dark):
            if fg not in scheme or bg not in scheme:
                raise SystemExit(f"make-brand: tokens.css has no --{fg} or --{bg}")
        rows.append((f"{fg} on {bg}", ratio(light[fg], light[bg]), ratio(dark[fg], dark[bg]), need))
    return rows


def contrast_table() -> str:
    lines = ["| Pair | Light | Dark | Needs |", "|---|---|---|---|"]
    for name, lo, hi, need in contrast_rows():
        lines.append(f"| {name} | {lo:.2f} | {hi:.2f} | {need:g} |")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    if "--contrast" in argv:
        print(contrast_table())
        return 0
    check = "--check" in argv
    stale = []
    for path, content in outputs().items():
        data = content.encode("utf-8") if isinstance(content, str) else content
        old = path.read_bytes() if path.exists() else None
        if old is None or canonical(old) != canonical(data):
            stale.append(path)
            if not check:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
                print(f"wrote {path.relative_to(ROOT)}")
    weak = [(n, lo, hi, need) for n, lo, hi, need in contrast_rows() if min(lo, hi) < need]
    for n, lo, hi, need in weak:
        sys.stderr.write(f"make-brand: {n} is {min(lo, hi):.2f}:1, needs {need:g}:1\n")
    if check and stale:
        for p in stale:
            sys.stderr.write(f"make-brand: {p.relative_to(ROOT)} is out of date\n")
        return 1
    if weak:
        return 1
    if not stale:
        print("make-brand: nothing to change")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
