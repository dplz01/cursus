# Cursus for Android

Native Kotlin + Jetpack Compose app over the shared Rust core.

```text
Compose UI (app/src/main/java/chat/cursus)      screens, theme, notifications, Keystore
   │  Native.call(json) / Native.nextEvent()
android/jni  (libcursus_android.so)             4 JNI functions, logcat logging
   │
bridge/      (cursus-bridge)                    JSON API — the same one the macOS app uses
   │
core/        (cursus-core)                      session, commands, models, shared rules
   │  dlopen
libsimplex.so                                   official SimpleX Chat core (arm64-v8a)
```

No messaging logic lives in Kotlin: chats, messages, links, safety rules
and preferences come from `cursus-core`; the app keeps only how they are
shown (and Android duties: notifications, Keystore, lifecycle). Models in
`core/Models.kt` mirror `bridge/src/dto.rs`; the methods are in
`bridge/src/methods.rs`.

## Build

Needs: JDK 17+, the Android SDK (platform 35, build-tools 35) and NDK 27,
Rust with the `aarch64-linux-android` target and `cargo-ndk`.

```sh
rustup target add aarch64-linux-android
cargo install cargo-ndk
export ANDROID_HOME=~/Android/Sdk ANDROID_NDK_HOME=~/Android/Sdk/ndk/27.2.12479018

./android/fetch-libsimplex.sh        # official core from the pinned SimpleX APK (SHA-256 checked)
cd android && ./gradlew assembleDebug   # also builds the Rust side via cargo-ndk
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb logcat -s Cursus                  # core logs
```

`./gradlew assembleRelease` builds an optimised APK (debug-signed until
release signing is set up).

**64-bit ARM only** — SimpleX publishes its Android core for arm64-v8a.
x86_64 emulator images from API 30 run it through Android's ARM
translation, which is enough for UI work.

## Icons

`app/src/main/res/drawable/ic_*.xml` are generated from
`linux/assets/icons/*.svg` (`python3 android/tools/svg2vd.py`) — same
glyphs as every Cursus app; attribution in
`linux/assets/icons/ATTRIBUTION.md`.

## Status

Done: create profile / unlock (password optionally in the Android
Keystore), Chats and Channels lists, conversation (rich text, quotes,
images, files, reactions, reply / edit / delete), sending files and
voice messages (record → draft → send), downloads (with the unknown-relay
check), voice playback with real waveforms, photo viewer, contact
requests, group invitations, the New page (one-time invite, address,
join with a link, new group / channel overlay), contact pages and chat
preferences, group / channel pages (profile editing, members, roles,
invite, group link, preferences, support chats, leave / delete), settings
(profile, address, theme, privacy, notifications), receiving in the
background (foreground service), phone and tablet layouts.

Next: forward / message info / reaction members, emoji shortcodes,
chat lists (tags), profile pictures, app lock timeout, calls.

UI rule: every screen follows the Linux app's structure, cards and
wording (see `docs/DESIGN_SYSTEM.md` and `linux/src/ui.rs`).
