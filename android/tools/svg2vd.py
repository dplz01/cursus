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
            if tag == "g":
                t = child.get("transform")
                if t:
                    m = re.fullmatch(r"\s*matrix\(([^)]*)\)\s*", t)
                    vals = [float(v) for v in re.split(r"[\s,]+", m.group(1).strip())] if m else None
                    if not vals or vals[1] != 0 or vals[2] != 0:
                        sys.exit(f"{path}: unsupported transform {t}")
                    lines.append(f'{indent}<group android:scaleX="{fmt(vals[0])}" android:scaleY="{fmt(vals[3])}" '
                                 f'android:translateX="{fmt(vals[4])}" android:translateY="{fmt(vals[5])}">')
                    walk(child, a, indent + "    ")
                    lines.append(f"{indent}</group>")
                else:
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

    walk(svg, attrs_of(svg, {}), "        " if vb[0] or vb[1] else "    ")
    body = "\n".join(lines)
    if vb[0] or vb[1]:
        body = (f'    <group android:translateX="{fmt(-vb[0])}" android:translateY="{fmt(-vb[1])}">\n'
                f"{body}\n    </group>")
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            f"<!-- Generated by android/tools/svg2vd.py from linux/assets/icons/{os.path.basename(path)} -->\n"
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="24dp" android:height="24dp"\n'
            f'    android:viewportWidth="{fmt(vb[2])}" android:viewportHeight="{fmt(vb[3])}">\n'
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
