#!/usr/bin/env python3
"""Convert the Cursus icon set (linux/assets/icons/*.svg) to Android
VectorDrawables, so every platform ships the same glyphs.

    python3 android/tools/svg2vd.py            # writes app/src/main/res/drawable/ic_*.xml

Handles exactly what the icon set uses: <path>, <circle>, <rect>, <line>,
<g> (with an optional `matrix(a 0 0 d e f)` transform), presentation
attributes, `style="…"` and simple `.class{…}` rules. Colours are dropped
to black: the app tints every icon with a theme token.
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "linux", "assets", "icons")
OUT = os.path.join(ROOT, "android", "app", "src", "main", "res", "drawable")

INHERITED = ("fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin",
             "stroke-miterlimit", "fill-rule")


def local(tag):
    return tag.split("}", 1)[-1]


def parse_style(text):
    out = {}
    for part in (text or "").split(";"):
        if ":" in part:
            k, v = part.split(":", 1)
            out[k.strip()] = v.strip()
    return out


def num(v, default=0.0):
    if v is None:
        return default
    return float(re.sub(r"[a-z%]+$", "", v.strip()))


def fmt(x):
    s = f"{x:.4f}".rstrip("0").rstrip(".")
    return s if s not in ("-0", "") else "0"


def shape_to_path(tag, a):
    if tag == "path":
        return a.get("d")
    if tag == "circle" or tag == "ellipse":
        cx, cy = num(a.get("cx")), num(a.get("cy"))
        rx = num(a.get("r", a.get("rx")))
        ry = num(a.get("r", a.get("ry")))
        return (f"M{fmt(cx - rx)},{fmt(cy)}a{fmt(rx)},{fmt(ry)} 0 1,0 {fmt(2 * rx)},0"
                f"a{fmt(rx)},{fmt(ry)} 0 1,0 {fmt(-2 * rx)},0Z")
    if tag == "line":
        return f"M{fmt(num(a.get('x1')))},{fmt(num(a.get('y1')))}L{fmt(num(a.get('x2')))},{fmt(num(a.get('y2')))}"
    if tag == "rect":
        x, y = num(a.get("x")), num(a.get("y"))
        w, h = num(a.get("width")), num(a.get("height"))
        rx = num(a.get("rx", a.get("ry")), 0.0)
        ry = num(a.get("ry", a.get("rx")), 0.0)
        rx, ry = min(rx, w / 2), min(ry, h / 2)
        if rx == 0 and ry == 0:
            return f"M{fmt(x)},{fmt(y)}h{fmt(w)}v{fmt(h)}h{fmt(-w)}Z"
        return (f"M{fmt(x + rx)},{fmt(y)}h{fmt(w - 2 * rx)}a{fmt(rx)},{fmt(ry)} 0 0 1 {fmt(rx)},{fmt(ry)}"
                f"v{fmt(h - 2 * ry)}a{fmt(rx)},{fmt(ry)} 0 0 1 {fmt(-rx)},{fmt(ry)}"
                f"h{fmt(-(w - 2 * rx))}a{fmt(rx)},{fmt(ry)} 0 0 1 {fmt(-rx)},{fmt(-ry)}"
                f"v{fmt(-(h - 2 * ry))}a{fmt(rx)},{fmt(ry)} 0 0 1 {fmt(rx)},{fmt(-ry)}Z")
    return None



def axis_aligned(t):
    """`matrix(a 0 0 d e f)` or a `translate(..)` / `scale(..)` chain, as
    [a, 0, 0, d, e, f]; None for anything with rotation or skew."""
    a, d, e, f = 1.0, 1.0, 0.0, 0.0
    for name, args in re.findall(r"(\w+)\(([^)]*)\)", t):
        v = [float(x) for x in re.split(r"[\s,]+", args.strip()) if x]
        if name == "matrix" and len(v) == 6:
            if v[1] or v[2]:
                return None
            na, nd, ne, nf = v[0], v[3], v[4], v[5]
        elif name == "translate" and v:
            na, nd, ne, nf = 1.0, 1.0, v[0], v[1] if len(v) > 1 else 0.0
        elif name == "scale" and v:
            na, nd, ne, nf = v[0], v[1] if len(v) > 1 else v[0], 0.0, 0.0
        else:
            return None
        # current ∘ new (SVG applies the rightmost transform first)
        a, d, e, f = a * na, d * nd, a * ne + e, d * nf + f
    return [a, 0.0, 0.0, d, e, f]

def convert(path):
    tree = ET.parse(path)
    svg = tree.getroot()
    classes = {}
    for el in svg.iter():
        if local(el.tag) == "style" and el.text:
            for name, body in re.findall(r"\.([\w-]+)\s*\{([^}]*)\}", el.text):
                classes[name] = parse_style(body)

    vb = [float(v) for v in re.split(r"[\s,]+", svg.get("viewBox").strip())]
    lines = []

    def attrs_of(el, inherited):
        a = dict(inherited)
        for k in INHERITED:
            if el.get(k) is not None:
                a[k] = el.get(k)
        for cls in (el.get("class") or "").split():
            a.update({k: v for k, v in classes.get(cls, {}).items() if k in INHERITED})
        a.update({k: v for k, v in parse_style(el.get("style")).items() if k in INHERITED})
        return a

    def walk(el, inherited, indent):
        for child in el:
            tag = local(child.tag)
            if tag in ("defs", "style", "title", "desc", "metadata"):
                continue
            a = attrs_of(child, inherited)
            t = child.get("transform")
            if t:
                # Any element may carry a transform: wrap it in a <group>.
                vals = axis_aligned(t)
                if not vals or vals[1] != 0 or vals[2] != 0:
                    sys.exit(f"{path}: unsupported transform {t}")
                lines.append(f'{indent}<group android:scaleX="{vals[0]:.8g}" android:scaleY="{vals[3]:.8g}" '
                             f'android:translateX="{fmt(vals[4])}" android:translateY="{fmt(vals[5])}">')
                inner = ET.Element(child.tag, {k: v for k, v in child.attrib.items() if k != "transform"})
                inner.extend(list(child))
                inner.text = child.text
                holder = ET.Element("holder")
                holder.append(inner)
                walk(holder, inherited, indent + "    ")
                lines.append(f"{indent}</group>")
                continue
            if tag == "g":
                walk(child, a, indent)
                continue
            d = shape_to_path(tag, {**child.attrib})
            if d is None:
                sys.exit(f"{path}: unsupported element <{tag}>")
            d = " ".join(d.split())
            fill = a.get("fill", "#000")
            stroke = a.get("stroke", "none")
            out = [f'{indent}<path android:pathData="{d}"']
            if fill != "none":
                out.append(f'{indent}    android:fillColor="#FF000000"')
                if a.get("fill-rule") == "evenodd":
                    out.append(f'{indent}    android:fillType="evenOdd"')
            if stroke != "none":
                out.append(f'{indent}    android:strokeColor="#FF000000"')
                out.append(f'{indent}    android:strokeWidth="{fmt(num(a.get("stroke-width"), 1.0))}"')
                cap = a.get("stroke-linecap")
                if cap in ("round", "square"):
                    out.append(f'{indent}    android:strokeLineCap="{cap}"')
                join = a.get("stroke-linejoin")
                if join in ("round", "bevel"):
                    out.append(f'{indent}    android:strokeLineJoin="{join}"')
                if a.get("stroke-miterlimit"):
                    out.append(f'{indent}    android:strokeMiterLimit="{fmt(num(a.get("stroke-miterlimit")))}"')
            out[-1] += " />"
            lines.extend(out)

    # The icon set uses tight, non-square viewBoxes. A VectorDrawable stretches
    # its viewport to fill width × height, so pad the viewport to a square
    # and centre the glyph (what GTK does on Linux) instead of squashing it.
    side = max(vb[2], vb[3])
    tx = -vb[0] + (side - vb[2]) / 2
    ty = -vb[1] + (side - vb[3]) / 2
    moved = abs(tx) > 1e-9 or abs(ty) > 1e-9
    walk(svg, attrs_of(svg, {}), "        " if moved else "    ")
    body = "\n".join(lines)
    if moved:
        body = (f'    <group android:translateX="{fmt(tx)}" android:translateY="{fmt(ty)}">\n'
                f"{body}\n    </group>")
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            f"<!-- Generated by android/tools/svg2vd.py from linux/assets/icons/{os.path.basename(path)} -->\n"
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="24dp" android:height="24dp"\n'
            f'    android:viewportWidth="{fmt(side)}" android:viewportHeight="{fmt(side)}">\n'
            f"{body}\n</vector>\n")


def main():
    os.makedirs(OUT, exist_ok=True)
    count = 0
    for name in sorted(os.listdir(SRC)):
        if not name.endswith(".svg"):
            continue
        res = "ic_" + re.sub(r"[^a-z0-9]", "_", name[:-4].lower())
        with open(os.path.join(OUT, res + ".xml"), "w") as f:
            f.write(convert(os.path.join(SRC, name)))
        count += 1
    print(f"{count} icons → {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
