# Cursus — licenses and sources

Every Cursus package (Flatpak, AppImage) ships this folder.

| Component | License | Source |
|---|---|---|
| **Cursus** (this application) | AGPL-3.0-or-later — [`LICENSE`](../LICENSE) | https://github.com/dplz01/cursus (each release links its exact tag) |
| **libsimplex** — SimpleX Chat core 7.1.0.17 (`libsimplex.so`, `libHS*.so`, GHC runtime, `libffi.so.8`) | AGPL-3.0 | simplex-chat [`1fd7ed8327d8481299215ca3240eb0140400924a`](https://github.com/simplex-chat/simplex-chat/tree/1fd7ed8327d8481299215ca3240eb0140400924a) with simplexmq [`d1f97d38f7a01f112efb6be753a1b45a40777826`](https://github.com/simplex-chat/simplexmq/tree/d1f97d38f7a01f112efb6be753a1b45a40777826); prebuilt from [simplex-chat-libs `core-7.1.0.17`](https://github.com/simplex-chat/simplex-chat-libs/releases/tag/core-7.1.0.17). The Haskell libraries it links are listed in that source's `cabal.project` / `simplex-chat.cabal` with their own licenses. |
| **libopus** (statically linked, built from the copy bundled in the `audiopus_sys` crate) — Copyright 2001-2011 Xiph.Org, Skype Limited, Octasic, Jean-Marc Valin, Timothy B. Terriberry, CSIRO, Gregory Maxwell, Mark Borgerding, Erik de Castro Lopo | BSD-3-Clause — [`BSD-3-Clause-Opus.txt`](BSD-3-Clause-Opus.txt) | https://opus-codec.org · https://crates.io/crates/audiopus_sys |
| **Rust crates** compiled into Cursus (581) | MIT, Apache-2.0, BSD, MPL-2.0, Zlib, ISC, Unicode-3.0, CC0 … | [`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt) (list + license texts); sources on https://crates.io |
| **Noto Sans** (Regular, Bold, Italic) — Copyright 2022 The Noto Project Authors | SIL Open Font License 1.1 — [`OFL-1.1-Noto.txt`](OFL-1.1-Noto.txt) | https://github.com/notofonts/latin-greek-cyrillic |
| **Noto Color Emoji** (AppImage; Flatpak uses the runtime's copy) — Copyright 2013 Google LLC | SIL Open Font License 1.1 — [`OFL-1.1-Noto.txt`](OFL-1.1-Noto.txt) | https://github.com/googlefonts/noto-emoji |
| **UI icons** | MIT, CC BY, CC0, Public Domain (per icon) | [`ICONS-ATTRIBUTION.md`](ICONS-ATTRIBUTION.md) |

The Flatpak runtime (org.freedesktop.Platform) and the AppImage runtime are
not part of these packages' own code; they carry their own licenses.
