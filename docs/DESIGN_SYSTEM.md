# Cursus design system

**Canonical reference** for Linux, macOS, Android, and iOS.

> One product, one design language, one shared Rust brain, four native experiences.

Each platform implements these tokens and structures in its **native** UI toolkit. Native conventions win when they conflict with pixel-perfect matching. Do not ship a web shell inside four OS wrappers.

Older lowercase `design-system.md` redirects here; prefer this file.

## Product identity

| | |
|---|---|
| **Name** | Cursus |
| **Tone** | Calm, modern messenger — clear hierarchy, quiet chrome, content-first |
| **Not** | Dashboard clutter, neon glow, purple-gradient “AI” defaults, cross-platform HTML chrome |

## Architecture rule: shared structure, native chrome

Keep the **information architecture identical** on every platform:

1. **Chats**
2. **Channels**
3. **Calls**
4. **Settings**

The same four destinations, the same primary content models (conversation list → conversation, etc.), and the same responsive *modes* (mobile / compact / desktop).

Let each platform render **its own native navigation**:

| Platform | Desktop / wide | Compact / mobile |
|---|---|---|
| Linux (Iced) | Vertical nav rail | Bottom destination bar |
| macOS (SwiftUI) | `NavigationSplitView` / sidebar | Tab bar / stack |
| Android (Compose) | `NavigationRail` | `NavigationBar` |
| iOS (SwiftUI) | Sidebar / split | `TabView` + navigation stack |

That keeps Cursus recognizable without making it feel like one web app pasted onto four OSes. Shared Rust (`cursus-core` + `libsimplex`) owns session, commands, and models — not pixels.

## Color

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg.app` | `#F4F5F7` | `#12141A` | Window / root background |
| `bg.sidebar` | `#EBEDF0` | `#181B22` | Nav rail / bottom bar |
| `bg.list` | `#FFFFFF` | `#1C2028` | Middle column / lists |
| `bg.panel` | `#FFFFFF` | `#1C2028` | Conversation pane |
| `bg.elevated` | `#FFFFFF` | `#262B36` | Modals, incoming bubbles |
| `text.primary` | `#12141A` | `#F2F3F5` | Body / titles |
| `text.secondary` | `#5C6370` | `#A0A6B3` | Previews, timestamps |
| `text.muted` | `#8B919C` | `#6E7585` | Hints, disabled |
| `accent` | `#2F6FED` | `#5B8CFF` | Primary actions, selection |
| `accent.on` | `#FFFFFF` | `#0B1020` | Text on accent |
| `accent.soft` | accent @ 18–20% | accent @ 18–20% | Selected rows, outgoing bubbles |
| `border` | `#D7DBE2` | `#2E3440` | Hairlines |
| `danger` | `#C62828` | `#FF6B6B` | Destructive / errors |
| `success` | `#2E7D32` | `#6BCB77` | Sent / connected |

Selection and focus rings use `accent.soft`, not heavy shadows.

## Typography

| Role | Desktop | Mobile | Weight |
|---|---|---|---|
| App title | 18px | 17px | 600 |
| Section label | 12px | 12px | 600 |
| Chat name | 15px | 16px | 600 |
| Body / message | 14px | 16px | 400 |
| Preview / meta | 12px | 13px | 400 |

Prefer a readable **system UI** stack per platform (Inter/system on Linux, SF on Apple, Roboto/system on Android). Avoid decorative display fonts in chrome.

## Spacing and radius

| Token | Value |
|---|---|
| `space.1` … `space.6` | 4 / 8 / 12 / 16 / 24 / 32 px |
| `radius.sm` / `md` / `lg` / `pill` | 6 / 10 / 16 / 999 |

Message bubbles: `radius.lg`. Nav hit targets: min 40×40 desktop, 48×48 mobile (logical px).

## Layout breakpoints (logical pixels)

Breakpoints are defined in **logical / density-independent pixels** (DIP), never framebuffer / physical pixels.

On a 2× Retina or HiDPI Linux display, a window that is 600×800 *logical* is still **mobile**, even if the GPU buffer is 1200×1600. Scale factor is diagnostic only; layout math must not multiply by it.

Centralize thresholds per platform UI module (Linux: `linux/src/layout.rs`). Keep the numbers aligned across platforms:

| Name | Logical width | Layout |
|---|---|---|
| `mobile` | `< 650` | Full-screen destinations, platform bottom/tab nav, conversation is a push |
| `compact` | `650`–`950` | Bottom/tab nav + list; conversation replaces list with back |
| `desktop` | `> 950` | Persistent nav + list + conversation |

Width is primary; aspect ratio may nudge tall windows toward mobile. Resize, rotation, and safe-area changes must switch layouts **live** without restart, preserving section, selection, and session.

### Desktop structure

1. **Nav** — four destinations (platform-native rail / sidebar)
2. **List** — conversations / channels / calls / settings groups (~280–320 logical px)
3. **Main** — selected conversation or empty state

### Compact / mobile structure

- Platform bottom / tab navigation for the four sections
- List fills remaining area
- Opening a chat pushes a dedicated conversation view with clear back
- Adapt spacing and type via density tokens — do **not** squeeze the desktop chrome

## Icons

Use a single outlined icon language (stroke weight consistent). Prefer **vector paths** embedded or drawn by the toolkit so missing symbol fonts never show tofu.

Destinations: Chats (bubble), Channels (list rules), Calls (handset), Settings (gear).

## Motion

- 120–200ms ease for nav selection and list → conversation push
- No continuous decorative animation
- Respect OS “reduce motion”

## Empty states

One short sentence + one primary action max. No mascot spam.

## Theming

Light and dark from day one on desktop. Follow OS preference by default; Settings may override later.

## Checklist for new UI work

1. Does it preserve the four destinations?
2. Does it use tokens from this document?
3. Are breakpoints logical-px only?
4. Is navigation native to the platform?
5. Would the screen still feel like Cursus if the OS chrome were swapped?
