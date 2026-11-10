#!/usr/bin/env python3
"""Generate the third-party license notices every Cursus build ships.

Binary releases must carry the license notices of everything bundled in
them. Three files under licenses/, each covering all three apps (every
entry says which builds contain it):

  THIRD_PARTY_LICENSES.txt   Rust crates compiled into Cursus for Linux
                             (cursus-linux), macOS (cursus-bridge) and
                             Android (cursus-android), plus the Rust
                             standard library; license texts de-duplicated.
  SIMPLEX-CORE-LICENSES.txt  The official SimpleX core (libsimplex): the
                             Haskell packages it is built from (license
                             texts from Hackage) and the native libraries
                             it links (GHC runtime, GMP, OpenSSL, SQLCipher,
                             zstd, libffi, …).
  ANDROID-LIBRARIES.txt      Java/Kotlin libraries in the Android APK
                             (AndroidX, Jetpack Compose, Kotlin, kotlinx),
                             licenses from their Maven POMs.

Standard library only. Needs `cargo` with the crates fetched (any build
does that), the SimpleX libraries fetched (scripts/fetch-libs.sh and
android/fetch-libsimplex.sh), Gradle's cache populated (any Android
build), and network access to Hackage and the upstream repositories for
the core's license texts (cached in ~/.cache/cursus-licenses). Re-run after
dependency changes:

    ./scripts/third-party-licenses.py            # all three
    ./scripts/third-party-licenses.py rust       # just one: rust|core|android
"""

import html
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LICENSES = ROOT / "licenses"
CACHE = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "cursus-licenses"
SEP = "=" * 78
PREFIXES = ("license", "licence", "copying", "notice", "unlicense", "copyright")

# (app label, root package, targets it is built for)
RUST_APPS = [
    ("linux", "cursus-linux", ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]),
    ("macos", "cursus-bridge", ["aarch64-apple-darwin", "x86_64-apple-darwin"]),
    ("android", "cursus-android", ["aarch64-linux-android"]),
]


def write(name: str, parts: list[str]) -> None:
    out = LICENSES / name
    out.write_text("\n".join(parts).rstrip() + "\n")
    print(f"{out.relative_to(ROOT)} written")


def fetch(url: str) -> str | None:
    """GET with an on-disk cache; None on 404."""
    CACHE.mkdir(parents=True, exist_ok=True)
    key = CACHE / re.sub(r"[^A-Za-z0-9._-]", "_", url)
    if key.exists():
        text = key.read_text(errors="replace")
        return None if text == "\0404" else text
    req = urllib.request.Request(url, headers={"User-Agent": "cursus-license-notices"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            text = r.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            key.write_text("\0404")
            return None
        raise
    key.write_text(text)
    return text


# ---------------------------------------------------------------- Rust ----

def rust_crates(package: str, target: str) -> list[dict]:
    """Crates linked into `package` for `target` (normal deps, transitively)."""
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout)
    by_id = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    root = next(i for i in members if by_id[i]["name"] == package)
    seen, stack = set(), [root]
    while stack:
        node = nodes[stack.pop()]
        for dep in node["deps"]:
            # Build scripts and proc-macros run at build time, but a
            # proc-macro crate's normal edge is still listed: keep them.
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    return [by_id[i] for i in seen if i not in members]


def rust_std_notice() -> list[str]:
    docs = sorted(Path.home().glob(".rustup/toolchains/*/share/doc/rust/COPYRIGHT-library.html"))
    lines = [
        SEP, "Rust standard library (std, core, alloc and the crates they use)", SEP,
        "Copyright (c) The Rust Project Contributors. Licensed under MIT OR",
        "Apache-2.0 (texts below, under the crates that ship them).",
        "https://github.com/rust-lang/rust",
    ]
    if docs:
        text = docs[-1].read_text(errors="replace")
        text = re.sub(r"<(script|style)[^>]*>.*?</\1>", "", text, flags=re.S)
        text = re.sub(r"<br\s*/?>|</p>|</h\d>|</li>|</pre>", "\n", text)
        text = html.unescape(re.sub(r"<[^>]+>", "", text))
        text = re.sub(r"\n{3,}", "\n\n", text).strip()
        lines += ["", "The standard library's own copyright file (COPYRIGHT-library):", "", text]
    return lines + [""]


def workspace_packages() -> set[str]:
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--no-deps"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout)
    return {p["name"] for p in meta["packages"]}


def gen_rust() -> None:
    packages: dict[tuple[str, str], dict] = {}
    apps: dict[tuple[str, str], set[str]] = defaultdict(set)
    # A branch may not have every app yet (e.g. no Android JNI crate).
    present = workspace_packages()
    for label, package, targets in RUST_APPS:
        if package not in present:
            print(f"  ({package} not in this workspace: no {label} crates listed)")
            continue
        for target in targets:
            for pkg in rust_crates(package, target):
                key = (pkg["name"], pkg["version"])
                packages[key] = pkg
                apps[key].add(label)

    texts = defaultdict(list)
    rows, unknown = [], []
    order = [a for a, _, _ in RUST_APPS]
    for (name, version), pkg in sorted(packages.items()):
        lic = pkg.get("license") or (
            f"see {Path(pkg['license_file']).name}" if pkg.get("license_file") else None
        )
        if lic is None:
            unknown.append(f"{name} {version}")
            continue
        used = ", ".join(a for a in order if a in apps[(name, version)])
        rows.append(f"{name} {version}  —  {lic}  [{used}]")
        crate_dir = Path(pkg["manifest_path"]).parent
        files = sorted(f for f in crate_dir.iterdir() if f.is_file() and f.name.lower().startswith(PREFIXES))
        if pkg.get("license_file"):
            files.append(crate_dir / pkg["license_file"])
        for f in dict.fromkeys(files):
            try:
                text = f.read_text(errors="replace").strip()
            except OSError:
                continue
            if text:
                texts[text].append(f"{name} {version}")
    if unknown:
        sys.exit("crates without license metadata:\n  " + "\n  ".join(unknown))

    parts = [
        "Third-party Rust code in Cursus",
        "",
        "Cursus is licensed under AGPL-3.0-or-later (see LICENSE). The crates",
        "below are compiled into the Cursus apps under their own licenses; the",
        "brackets say which builds contain each one (linux = the Linux app,",
        "macos = the macOS app's core bridge, android = libcursus_android.so).",
        "",
        SEP, f"CRATES ({len(rows)})", SEP, *rows, "",
        *rust_std_notice(),
    ]
    for text, crates in sorted(texts.items(), key=lambda kv: (kv[1][0], kv[0])):
        parts += [SEP, "Used by: " + ", ".join(crates), SEP, text, ""]
    write("THIRD_PARTY_LICENSES.txt", parts)


# ---------------------------------------------------- SimpleX core ----

LINUX_CORE = ROOT / "third_party" / "libsimplex"
ANDROID_CORE = ROOT / "android" / "app" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libsimplex.so"

# Parts of the SimpleX core itself: AGPL-3.0, covered by LICENSE/NOTICE.md.
SIMPLEX_OWN = {"simplex-chat", "simplexmq"}
# GHC's boot packages that Android links statically under short unit ids.
GHC_BOOT = {
    "base": "4.18.1.0", "ghc-prim": "0.10.0", "ghc-bignum": "1.3",
    "template-haskell": "2.20.0.0", "ghc-boot-th": "9.6.3",
}

# Native code linked into libsimplex — statically (Android; GMP also on
# macOS), shipped next to it (libffi everywhere, OpenSSL's libcrypto on
# macOS), or compiled in from the C sources bundled in Haskell packages.
NATIVE = [
    ("GHC runtime system (rts) — The Glasgow Haskell Compiler, 9.6.3",
     "BSD-3-Clause (The Glasgow Haskell Compiler License)", "all builds",
     "https://gitlab.haskell.org/ghc/ghc/-/raw/ghc-9.6.3-release/LICENSE"),
    ("GNU MP (GMP) — Free Software Foundation, https://gmplib.org",
     "LGPL-3.0-or-later (used under it; GMP is dual LGPL-3.0+ / GPL-2.0+)",
     "Android (static), macOS (static in libHSghc-bignum); Linux uses the system's",
     "https://www.gnu.org/licenses/lgpl-3.0.txt"),
    ("  (GNU GPL v3, which the LGPL v3 above supplements)", "GPL-3.0", "",
     "https://www.gnu.org/licenses/gpl-3.0.txt"),
    ("OpenSSL 3 (libcrypto)", "Apache-2.0",
     "Android (static), macOS (libcrypto.3.0.dylib); Linux uses the system's",
     "https://raw.githubusercontent.com/openssl/openssl/openssl-3.0.0/LICENSE.txt"),
    ("libffi", "MIT",
     "all builds (libffi.so.8 / libffi.dylib; static on Android)",
     "https://raw.githubusercontent.com/libffi/libffi/master/LICENSE"),
    ("SQLCipher — Zetetic LLC", "BSD-3-Clause",
     "all builds (via direct-sqlcipher)",
     "https://raw.githubusercontent.com/sqlcipher/sqlcipher/master/LICENSE.md"),
    ("SQLite", "Public domain", "all builds (inside SQLCipher)", None),
    ("Zstandard (zstd)", "BSD-3-Clause",
     "all builds (via the zstd package)",
     "https://raw.githubusercontent.com/facebook/zstd/dev/LICENSE"),
    ("libjpeg-turbo", "IJG License, BSD-3-Clause, zlib License", "all builds (via jpeg-turbo)",
     "https://raw.githubusercontent.com/libjpeg-turbo/libjpeg-turbo/main/LICENSE.md"),
    ("libspng", "BSD-2-Clause", "all builds (via JuicyPixels-spng)",
     "https://raw.githubusercontent.com/randy408/libspng/master/LICENSE"),
    ("stb_image_resize — Sean Barrett", "MIT or public domain (Unlicense)",
     "all builds (via JuicyPixels-stbir)",
     "https://raw.githubusercontent.com/nothings/stb/master/LICENSE"),
    ("zlib — Jean-loup Gailly and Mark Adler", "Zlib",
     "Android (static, via the zlib package); desktop uses the system's",
     "https://raw.githubusercontent.com/madler/zlib/master/LICENSE"),
    ("LibYAML", "MIT",
     "all builds (via libyaml)",
     "https://raw.githubusercontent.com/yaml/libyaml/master/License"),
]


def zdecode(s: str) -> str:
    out, i = [], 0
    table = {"m": "-", "i": ".", "u": "_", "z": "z", "d": "$"}
    while i < len(s):
        if s[i] == "z" and i + 1 < len(s) and s[i + 1] in table:
            out.append(table[s[i + 1]]); i += 2
        else:
            out.append(s[i]); i += 1
    return "".join(out)


def core_packages() -> dict[tuple[str, str], set[str]]:
    """Haskell packages per build: Linux from the shipped libHS*.so names
    (macOS ships the same core tag), Android from the unit ids in its
    statically linked libsimplex.so."""
    found: dict[tuple[str, str], set[str]] = defaultdict(set)
    if not LINUX_CORE.is_dir():
        sys.exit("third_party/libsimplex missing: run scripts/fetch-libs.sh")
    for f in os.listdir(LINUX_CORE):
        m = re.match(r"libHS(.+?)-(\d+(?:\.\d+)*)(?:-l-[^-]+)?(?:-[0-9a-f]{16,})?-ghc[\d.]+\.so$", f)
        if m:
            found[(m.group(1), m.group(2))].update({"linux", "macos"})
    if not (ROOT / "android" / "app").is_dir():
        print("  (no Android app in this tree: Android core not scanned)")
        return finish_core(found)
    if not ANDROID_CORE.is_file():
        sys.exit("Android libsimplex.so missing: run android/fetch-libsimplex.sh")
    strings = subprocess.run(["strings", "-n", "6", str(ANDROID_CORE)],
                             capture_output=True, text=True, check=True).stdout
    for m in re.finditer(r"(?<![A-Za-z0-9])([A-Za-z][A-Za-z0-9]*(?:zm[A-Za-z][A-Za-z0-9]*)*)zm(\d+(?:zi\d+)*)(?:zm|_)", strings):
        name, ver = zdecode(m.group(1)), zdecode(m.group(2))
        if name.startswith(("ghc_wrapper", "zd")) or "." in name or "$" in name:
            continue
        found[(name, ver)].add("android")
    for name in GHC_BOOT:
        found[(name, GHC_BOOT[name])].add("android")
    return finish_core(found)


def finish_core(found: dict[tuple[str, str], set[str]]) -> dict[tuple[str, str], set[str]]:
    for name, ver in GHC_BOOT.items():
        found[(name, ver)].update({"linux", "macos"})
    # The runtime is listed under NATIVE (GHC's own license).
    found.pop(("rts", "1.0.2"), None)
    return found


# Forks the SimpleX core builds instead of the Hackage release, from
# simplex-chat's cabal.project at the pinned commit: (repo, tag, subdir).
CORE_FORKS = {
    "socks": ("simplex-chat/hs-socks", "a30cc7a79a08d8108316094f8f2f82a0c5e1ac51", ""),
    "direct-sqlcipher": ("simplex-chat/direct-sqlcipher", "2330df3dc8c4b660674d3c02d76bb86f66c2abee", ""),
    "sqlcipher-simple": ("simplex-chat/sqlcipher-simple", "a46bd361a19376c5211f1058908fc0ae6bf42446", ""),
    "aeson": ("simplex-chat/aeson", "aab7b5a14d6c5ea64c64dcaee418de1bb00dcc2b", ""),
    "terminal": ("simplex-chat/haskell-terminal", "f708b00009b54890172068f168bf98508ffcd495", ""),
    "zip": ("simplex-chat/zip", "2eff156c3aac389e35d38bf10a52733d7061640a", ""),
    "JuicyPixels-jpeg-turbo": ("simplex-chat/hs-jpeg-turbo", "d0a4d954038ee5bcb5c4dc9372a6fb0b05fed77c", "JuicyPixels-jpeg-turbo/"),
    "JuicyPixels-spng": ("simplex-chat/hs-jpeg-turbo", "d0a4d954038ee5bcb5c4dc9372a6fb0b05fed77c", "JuicyPixels-spng/"),
    "jpeg-turbo": ("simplex-chat/hs-jpeg-turbo", "d0a4d954038ee5bcb5c4dc9372a6fb0b05fed77c", "jpeg-turbo/"),
    "JuicyPixels-stbir": ("simplex-chat/JuicyPixels-stbir", "e0090e14f6348a33b53c11b377cb69df766b02b4", ""),
}
# MIT, as written in the .cabal file of packages that ship no license
# file at all (e.g. iso8601-time); the holder comes from the .cabal.
MIT_TEMPLATE = """MIT License

Copyright (c) {holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""
# Licenses that need no text (dedications, not grants with conditions).
NO_TEXT = {"publicdomain", "public domain", "cc0-1.0", "unlicense"}


def hackage_license(name: str, ver: str) -> tuple[str, list[tuple[str, str]]]:
    if name in CORE_FORKS:
        repo, tag, sub = CORE_FORKS[name]
        base = f"https://raw.githubusercontent.com/{repo}/{tag}/{sub}"
    else:
        base = f"https://hackage.haskell.org/package/{name}-{ver}/src/"
    cabal = fetch(base + f"{name}.cabal") if name in CORE_FORKS else \
        fetch(f"https://hackage.haskell.org/package/{name}-{ver}/{name}.cabal")
    if cabal is None:
        return "not found", []
    lic = re.search(r"^license\s*:\s*(.+)$", cabal, re.M | re.I)
    files: list[str] = []
    for m in re.finditer(r"^license-files?\s*:\s*((?:.+\n?)(?:^\s+.+\n?)*)", cabal, re.M | re.I):
        files += [f.strip(",") for f in m.group(1).split() if f.strip(",")]
    texts = []
    for f in files or ["LICENSE"]:
        t = fetch(base + f)
        if t and t.strip():
            texts.append((f, t.strip()))
    license_name = lic.group(1).strip() if lic else "see license file"
    if not texts and license_name.upper() == "MIT":
        holder = re.search(r"^(?:copyright|author)\s*:\s*(.+)$", cabal, re.M | re.I)
        if holder:
            license_name += " (no license file upstream; standard MIT text below)"
            texts.append(("LICENSE", MIT_TEMPLATE.format(holder=holder.group(1).strip())))
    if name in CORE_FORKS:
        repo, tag, _ = CORE_FORKS[name]
        license_name += f" (SimpleX fork: github.com/{repo} @ {tag[:7]})"
    return license_name, texts


def gen_core() -> None:
    pkgs = core_packages()
    rows, texts, missing = [], defaultdict(list), []
    order = ["linux", "macos", "android"]
    for (name, ver) in sorted(pkgs, key=lambda k: (k[0].lower(), k[1])):
        used = ", ".join(a for a in order if a in pkgs[(name, ver)])
        if name in SIMPLEX_OWN:
            rows.append(f"{name} {ver}  —  AGPL-3.0 (SimpleX Chat; see NOTICE.md)  [{used}]")
            continue
        lic, files = hackage_license(name, ver)
        rows.append(f"{name} {ver}  —  {lic}  [{used}]")
        if not files and lic.split(" (")[0].strip().lower() not in NO_TEXT:
            missing.append(f"{name} {ver}")
        for _, t in files:
            texts[t].append(f"{name} {ver}")
    if missing:
        sys.exit("no license text found for:\n  " + "\n  ".join(missing))

    parts = [
        "Third-party code in the SimpleX core (libsimplex)",
        "",
        "Every Cursus app runs the official SimpleX Chat core, AGPL-3.0 (see",
        "NOTICE.md for its exact source). It is built with GHC from the Haskell",
        "packages below and links the native libraries after them; each keeps",
        "its own license. Brackets say which builds contain each package",
        "(Linux and macOS ship it as libHS*.so / .dylib next to libsimplex;",
        "Android links it into libsimplex.so). Sources: https://hackage.haskell.org",
        "",
        SEP, f"HASKELL PACKAGES ({len(rows)})", SEP, *rows, "",
        SEP, "NATIVE LIBRARIES", SEP,
    ]
    for title, lic, where, _ in NATIVE:
        if where:
            parts.append(f"{title.strip()}  —  {lic}  [{where}]")
    parts.append("")
    for title, lic, where, url in NATIVE:
        if not url:
            continue
        t = fetch(url)
        if not t:
            sys.exit(f"license text not found: {url}")
        parts += [SEP, title.strip(), f"Source of this text: {url}", SEP, t.strip(), ""]
    for text, users in sorted(texts.items(), key=lambda kv: (kv[1][0].lower(), kv[0])):
        parts += [SEP, "Used by: " + ", ".join(users), SEP, text, ""]
    write("SIMPLEX-CORE-LICENSES.txt", parts)


# ----------------------------------------------------------- Android ----

APACHE_URL = "https://www.apache.org/licenses/LICENSE-2.0.txt"


def maven_deps() -> list[tuple[str, str, str]]:
    out = subprocess.run(
        ["./gradlew", "-q", ":app:dependencies", "--configuration", "releaseRuntimeClasspath"],
        cwd=ROOT / "android", check=True, capture_output=True, text=True,
    ).stdout
    deps = set()
    for line in out.splitlines():
        m = re.match(r"^[+\\| -]+([\w.\-]+):([\w.\-]+)(?::([\w.\-]+))?(?: -> ([\w.\-]+))?", line)
        if not m or line.lstrip().startswith("project "):
            continue
        g, a, v, resolved = m.groups()
        v = resolved or v
        if v:
            deps.add((g, a, v))
    return sorted(deps)


def pom_licenses(g: str, a: str, v: str, depth: int = 0) -> list[tuple[str, str]]:
    base = Path.home() / ".gradle/caches/modules-2/files-2.1" / g / a / v
    poms = sorted(base.glob(f"*/{a}-{v}.pom")) if base.is_dir() else []
    if not poms:
        return []
    ns = {"m": "http://maven.apache.org/POM/4.0.0"}
    root = ET.parse(poms[0]).getroot()
    found = [((l.findtext("m:name", "", ns) or "").strip(), (l.findtext("m:url", "", ns) or "").strip())
             for l in root.findall("m:licenses/m:license", ns)]
    if found or depth > 4:
        return found
    parent = root.find("m:parent", ns)
    if parent is not None:
        return pom_licenses(parent.findtext("m:groupId", "", ns), parent.findtext("m:artifactId", "", ns),
                            parent.findtext("m:version", "", ns), depth + 1)
    return []


def jar_notices(g: str, a: str, v: str) -> list[str]:
    """NOTICE files inside a library's jar/aar (Apache-2.0 §4(d) asks to pass
    them on); LICENSE copies are skipped — the license text is below."""
    import io
    import zipfile
    base = Path.home() / ".gradle/caches/modules-2/files-2.1" / g / a / v
    found = []
    for f in sorted(base.glob("*/*.jar")) + sorted(base.glob("*/*.aar")):
        if f.name.endswith(("-sources.jar", "-javadoc.jar")):
            continue
        try:
            archives = [zipfile.ZipFile(f)]
        except zipfile.BadZipFile:
            continue
        for n in archives[0].namelist():
            if n.endswith("classes.jar"):
                archives.append(zipfile.ZipFile(io.BytesIO(archives[0].read(n))))
        for z in archives:
            for n in z.namelist():
                if re.search(r"(^|/)NOTICE[^/]*$", n, re.I):
                    found.append(z.read(n).decode("utf-8", errors="replace").strip())
    return found


def gen_android() -> None:
    if not (ROOT / "android" / "app").is_dir():
        print("  (no Android app in this tree: ANDROID-LIBRARIES.txt not generated)")
        return
    rows, missing, names = [], [], set()
    notices: dict[str, list[str]] = defaultdict(list)
    for g, a, v in maven_deps():
        for n in jar_notices(g, a, v):
            notices[n].append(f"{g}:{a}:{v}")
        lic = pom_licenses(g, a, v)
        if not lic:
            missing.append(f"{g}:{a}:{v}")
            continue
        rows.append(f"{g}:{a}:{v}  —  " + "; ".join(f"{n} <{u}>" if u else n for n, u in lic))
        names.update(n for n, _ in lic)
    if missing:
        sys.exit("no license in POM for:\n  " + "\n  ".join(missing))
    unexpected = {n for n in names if "apache" not in n.lower()}
    if unexpected:
        sys.exit(f"add the text for these licenses to gen_android(): {sorted(unexpected)}")
    parts = [
        "Third-party Java/Kotlin libraries in Cursus for Android",
        "",
        "The Android app is built with these libraries (AndroidX, Jetpack",
        "Compose, the Kotlin standard library and kotlinx), copyright The",
        "Android Open Source Project, JetBrains s.r.o. and their contributors,",
        "each under the license named in its Maven POM. Sources: https://maven.google.com",
        "and https://central.sonatype.com.",
        "",
        SEP, f"LIBRARIES ({len(rows)})", SEP, *rows, "",
        SEP, "Apache License 2.0", SEP, fetch(APACHE_URL).strip(), "",
    ]
    for text, users in sorted(notices.items(), key=lambda kv: kv[1][0]):
        parts += [SEP, "NOTICE from: " + ", ".join(users), SEP, text, ""]
    write("ANDROID-LIBRARIES.txt", parts)


def main() -> int:
    which = sys.argv[1:] or ["rust", "core", "android"]
    for w in which:
        {"rust": gen_rust, "core": gen_core, "android": gen_android}[w]()
    return 0


if __name__ == "__main__":
    sys.exit(main())
