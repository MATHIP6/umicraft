#!/usr/bin/env python3
"""Rasterizes the SVG icons of the UI into crates/umicraft-ui/assets/icons.r8.

The icons are Phosphor Icons (regular weight, MIT, see
crates/umicraft-ui/assets/icons/LICENSE-phosphor.txt): one filled path each on
a 256 unit grid. The output is one 48x48 coverage bitmap (one byte per texel)
per icon, in the order of `Icon::ALL` in crates/umicraft-ui/src/icons.rs.

Usage: python docs/tools/gen_icons.py
"""

import math
import re
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

ROOT = Path(__file__).resolve().parents[2] / "crates" / "umicraft-ui"
SIDE = 48
SUPERSAMPLE = 8
GRID = 256.0
# The order of `Icon::ALL`.
ORDER = ["grid", "sliders", "search", "close", "gauge", "target", "cursor", "signal", "run", "blur", "gear", "lightbulb"]

TOKEN = re.compile(r"([MmLlHhVvCcSsQqAaZz])|(-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?)")


def arc_to_points(p0, rx, ry, phi, large, sweep, p1, steps=32):
    """Flattens an SVG elliptical arc (endpoint form) into points."""
    (x1, y1), (x2, y2) = p0, p1
    if rx == 0 or ry == 0 or p0 == p1:
        return [p1]
    rx, ry = abs(rx), abs(ry)
    cos_p, sin_p = math.cos(math.radians(phi)), math.sin(math.radians(phi))
    dx, dy = (x1 - x2) / 2, (y1 - y2) / 2
    x1p, y1p = cos_p * dx + sin_p * dy, -sin_p * dx + cos_p * dy
    scale = x1p**2 / rx**2 + y1p**2 / ry**2
    if scale > 1:
        rx, ry = rx * math.sqrt(scale), ry * math.sqrt(scale)
    num = rx**2 * ry**2 - rx**2 * y1p**2 - ry**2 * x1p**2
    den = rx**2 * y1p**2 + ry**2 * x1p**2
    coef = math.sqrt(max(0.0, num / den)) * (-1 if large == sweep else 1)
    cxp, cyp = coef * rx * y1p / ry, -coef * ry * x1p / rx
    cx = cos_p * cxp - sin_p * cyp + (x1 + x2) / 2
    cy = sin_p * cxp + cos_p * cyp + (y1 + y2) / 2

    def angle(ux, uy, vx, vy):
        return math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)

    start = angle(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and delta > 0:
        delta -= 2 * math.pi
    elif sweep and delta < 0:
        delta += 2 * math.pi
    points = []
    for i in range(1, steps + 1):
        t = start + delta * i / steps
        ex, ey = rx * math.cos(t), ry * math.sin(t)
        points.append((cos_p * ex - sin_p * ey + cx, sin_p * ex + cos_p * ey + cy))
    return points


def cubic_points(p0, c1, c2, p1, steps=24):
    points = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        points.append(
            (
                u**3 * p0[0] + 3 * u**2 * t * c1[0] + 3 * u * t**2 * c2[0] + t**3 * p1[0],
                u**3 * p0[1] + 3 * u**2 * t * c1[1] + 3 * u * t**2 * c2[1] + t**3 * p1[1],
            )
        )
    return points


def quadratic_points(p0, c, p1, steps=16):
    points = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        points.append(
            (
                u**2 * p0[0] + 2 * u * t * c[0] + t**2 * p1[0],
                u**2 * p0[1] + 2 * u * t * c[1] + t**2 * p1[1],
            )
        )
    return points


def subpaths(d):
    """The closed polygons of a path `d`, curves flattened."""
    tokens = [(m.group(1), m.group(2)) for m in TOKEN.finditer(d)]
    result, current = [], []
    pos = start = (0.0, 0.0)
    last_control = None
    command = None
    i = 0

    def take(count):
        nonlocal i
        values = []
        for _ in range(count):
            if i >= len(tokens) or tokens[i][1] is None:
                sys.exit(f"path {d[:30]}...: expected {count} numbers")
            values.append(float(tokens[i][1]))
            i += 1
        return values

    while i < len(tokens):
        letter, _ = tokens[i]
        if letter:
            command = letter
            i += 1
            if command in "Zz":
                if current:
                    result.append(current)
                current, pos, last_control = [], start, None
                continue
        elif command is None:
            sys.exit("path starts with a number")
        relative = command.islower()
        up = command.upper()
        ox, oy = pos if relative else (0.0, 0.0)
        last = None
        if up == "M":
            x, y = take(2)
            if current:
                result.append(current)
            pos = (x + ox, y + oy)
            start, current = pos, [pos]
            command = "l" if relative else "L"
        elif up == "L":
            x, y = take(2)
            pos = (x + ox, y + oy)
            current.append(pos)
        elif up == "H":
            (x,) = take(1)
            pos = (x + ox, pos[1])
            current.append(pos)
        elif up == "V":
            (y,) = take(1)
            pos = (pos[0], y + oy)
            current.append(pos)
        elif up == "C":
            x1, y1, x2, y2, x, y = take(6)
            c1, c2, end = (x1 + ox, y1 + oy), (x2 + ox, y2 + oy), (x + ox, y + oy)
            current += cubic_points(pos, c1, c2, end)
            pos, last = end, c2
        elif up == "S":
            x2, y2, x, y = take(4)
            c1 = (2 * pos[0] - last_control[0], 2 * pos[1] - last_control[1]) if last_control else pos
            c2, end = (x2 + ox, y2 + oy), (x + ox, y + oy)
            current += cubic_points(pos, c1, c2, end)
            pos, last = end, c2
        elif up == "Q":
            x1, y1, x, y = take(4)
            control, end = (x1 + ox, y1 + oy), (x + ox, y + oy)
            current += quadratic_points(pos, control, end)
            pos = end
        elif up == "A":
            rx, ry, phi, large, sweep, x, y = take(7)
            end = (x + ox, y + oy)
            current += arc_to_points(pos, rx, ry, phi, int(large), int(sweep), end)
            pos = end
        else:
            sys.exit(f"unsupported command {command}")
        last_control = last
    if current:
        result.append(current)
    return result


def rasterize_stroked(d, grid, width):
    """A path drawn as a line of `width` with round caps and joins."""
    size = SIDE * SUPERSAMPLE
    scale = size / grid
    ink = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(ink)
    radius = width * scale / 2
    for points in subpaths(d):
        scaled = [(x * scale, y * scale) for x, y in points]
        for a, b in zip(scaled, scaled[1:]):
            draw.line([a, b], fill=255, width=round(radius * 2))
        for x, y in scaled:
            draw.ellipse([x - radius, y - radius, x + radius, y + radius], fill=255)
    return ink.resize((SIDE, SIDE), Image.BOX)


def rasterize(svg):
    d = re.search(r'\sd="([^"]+)"', svg).group(1)
    box = re.search(r'viewBox="[-\d.]+ [-\d.]+ ([\d.]+) [\d.]+"', svg)
    grid = float(box.group(1)) if box else GRID
    stroke = re.search(r'stroke-width="([\d.]+)"', svg)
    if stroke:
        return rasterize_stroked(d, grid, float(stroke.group(1)))
    size = SIDE * SUPERSAMPLE
    scale = size / grid
    ink = Image.new("1", (size, size), 0)
    for polygon in subpaths(d):
        if len(polygon) < 3:
            continue
        layer = Image.new("1", (size, size), 0)
        ImageDraw.Draw(layer).polygon([(x * scale, y * scale) for x, y in polygon], fill=1)
        # Even-odd: a path inside another cuts a hole, as in the Phosphor sources.
        ink = ImageChops.logical_xor(ink, layer)
    return ink.convert("L").resize((SIDE, SIDE), Image.BOX)


def main():
    out = bytearray()
    for name in ORDER:
        svg = (ROOT / "assets" / "icons" / f"{name}.svg").read_text(encoding="utf-8")
        out += rasterize(svg).tobytes()
    (ROOT / "assets" / "icons.r8").write_bytes(out)
    print(f"{len(ORDER)} icons, {len(out)} bytes")


if __name__ == "__main__":
    main()
