#!/usr/bin/env python3
"""Generate flatpak/cargo-sources.json from Cargo.lock (offline cargo build).

Flatpak builds have no network, so every crate is listed as a pinned
archive (crates.io tarball + the sha256 from Cargo.lock) that
flatpak-builder downloads and unpacks into cargo/vendor/, plus a cargo
config that points crates.io at that vendor directory.

Standard library only. Re-run after any Cargo.lock change:

    ./scripts/flatpak-cargo-sources.py
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
OUT = ROOT / "flatpak" / "cargo-sources.json"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"


def main() -> int:
    lock = tomllib.loads(LOCK.read_text())
    sources = []
    for pkg in lock.get("package", []):
        source = pkg.get("source")
        if source is None:
            continue  # workspace member
        if source != CRATES_IO:
            print(f"unsupported source for {pkg['name']}: {source}", file=sys.stderr)
            return 1
        name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
        dest = f"{VENDOR}/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        # cargo verifies vendored crates against this file; empty "files"
        # skips per-file hashes (the archive itself is already pinned).
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )
    sources.append(
        {
            "type": "inline",
            "contents": (
                '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n'
                '[source.crates-io]\nreplace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(sources, indent=2) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(sources)} entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
