Vector PDF templates of `linux/assets/icons/*.svg` (same glyphs, same
licences — see `linux/assets/icons/ATTRIBUTION.md`). Tinted at runtime.
Regenerate after changing an SVG:

    for f in linux/assets/icons/*.svg; do
      rsvg-convert -f pdf -w 48 -h 48 -o "macos/Resources/icons/$(basename "$f" .svg).pdf" "$f"
    done
