#!/usr/bin/env python3
"""Generate licenses/THIRD_PARTY_LICENSES.txt for the Rust dependencies.

Binary releases must carry the license notices of every bundled crate
(MIT, Apache-2.0, BSD, …). This lists each crate built into Cursus on
Linux (x86_64 + aarch64) with its license, followed by the license texts
the crates ship, de-duplicated.

Standard library only; needs `cargo` and the crates in the local registry
(any `cargo build` fetches them). Re-run after Cargo.lock changes:

    ./scripts/third-party-licenses.py
"""

import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "licenses" / "THIRD_PARTY_LICENSES.txt"
TARGETS = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]
PREFIXES = ("license", "licence", "copying", "notice", "unlicense", "copyright")


def metadata(target: str) -> dict:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def main() -> int:
    packages = {}
    for target in TARGETS:
        meta = metadata(target)
        by_id = {p["id"]: p for p in meta["packages"]}
        members = set(meta["workspace_members"])
        for node in meta["resolve"]["nodes"]:
            if node["id"] in members:
                continue
            pkg = by_id[node["id"]]
            packages[(pkg["name"], pkg["version"])] = pkg

    texts = defaultdict(list)  # license text -> crates shipping it
    rows = []
    for (name, version), pkg in sorted(packages.items()):
        license_expr = pkg.get("license") or (
            f"see {Path(pkg['license_file']).name}" if pkg.get("license_file") else "UNKNOWN"
        )
        rows.append(f"{name} {version}  —  {license_expr}")
        crate_dir = Path(pkg["manifest_path"]).parent
        files = sorted(
            f for f in crate_dir.iterdir() if f.is_file() and f.name.lower().startswith(PREFIXES)
        )
        if pkg.get("license_file"):
            files.append(crate_dir / pkg["license_file"])
        for f in dict.fromkeys(files):
            try:
                text = f.read_text(errors="replace").strip()
            except OSError:
                continue
            if text:
                texts[text].append(f"{name} {version}")

    unknown = [r for r in rows if r.endswith("UNKNOWN")]
    if unknown:
        print("crates without license metadata:", *unknown, sep="\n  ", file=sys.stderr)
        return 1

    sep = "=" * 78
    parts = [
        "Third-party Rust crates in Cursus",
        "",
        "Cursus is licensed under AGPL-3.0-or-later (see LICENSE). The crates",
        "below are compiled into the Cursus binary under their own licenses.",
        "",
        sep,
        f"CRATES ({len(rows)})",
        sep,
        *rows,
        "",
    ]
    for text, crates in sorted(texts.items(), key=lambda kv: (kv[1][0], kv[0])):
        parts += [sep, "Used by: " + ", ".join(crates), sep, text, ""]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(parts) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(rows)} crates, {len(texts)} distinct license texts")
    return 0


if __name__ == "__main__":
    sys.exit(main())
