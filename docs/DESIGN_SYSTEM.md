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

Exact values live in `linux/src/theme.rs` (`Tokens::light()` / `Tokens::dark()`); this table is the source of truth for every other platform.

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg.app` | `#F4F5F7` | `#12141A` | Window / root background |
| `bg.sidebar` | `#EBEDF0` | `#181B22` | Nav rail / bottom bar, **text-field fill**, secondary-button fill |
| `bg.list` | `#FFFFFF` | `#1C2028` | Middle column / lists |
| `bg.panel` | `#FFFFFF` | `#1C2028` | Conversation pane, settings pages |
| `bg.elevated` | `#FFFFFF` | `#262B36` | Menus, popups, incoming bubbles |
| `bg.hover` | `#E2E5EA` | `#262B36` | Hover fill for every quiet control |
| `text.primary` | `#12141A` | `#F2F3F5` | Body / titles |
| `text.secondary` | `#5C6370` | `#A0A6B3` | Previews, idle icons, placeholders in rounded fields |
| `text.muted` | `#8B919C` | `#6E7585` | Section labels, hints, disabled labels |
| `accent` | `#2F6FED` | `#5B8CFF` | Primary actions, selection, links |
| `accent.hover` / `accent.press` | `#255DD1` / `#1C4CB8` | `#6B99FF` / `#4D7AEB` | Primary button states |
| `accent.on` | `#FFFFFF` | `#0B1020` | Text / icons on accent |
| `accent.soft` | accent @ 16% | accent @ 18% | Selected rows, outgoing bubbles, unread badge, link box |
| `accent.soft.press` | accent @ 36% | accent @ 40% | Pressed soft surfaces |
| `border` | `#D7DBE2` | `#2E3440` | Hairlines (1px) |
| `danger` | `#C62828` | `#FF6B6B` | Destructive labels / icons |
| `success` | `#2E7D32` | `#6BCB77` | Sent / connected / accept |
| `gold` (`always`) | `#B58B0F` | `#ECD04F` | Pending / invited status, asymmetric “always” prefs, support badge |
| `disabled.fill` | `#D1D5DA` | `#404D66` | Disabled primary buttons |

Rules:

- Selection and focus use `accent.soft` / a 1px `accent` border — never heavy shadows or glows.
- Hover never fills an icon button solid; it only adds `bg.hover` behind it (icons stay readable).
- Status colour is semantic: gold = waiting on someone, green = done, red = destructive. Do not reuse gold for decoration.
- Touch devices: hover styles are suppressed after a touch (`hover_ok = false`) so nothing stays “stuck” highlighted.

## Typography

| Role | Desktop | Compact | Mobile | Weight |
|---|---|---|---|---|
| App / page title | 18px | 16px | 17px | 600 |
| Section label | = meta | = meta | = meta | 400, `text.muted` |
| Chat name / body / message | 14px | 14px | 16px | 400 |
| Preview / meta / buttons | 12px | 12px | 13px | 400 |

- Font: **Noto Sans** (Regular / Bold / Italic) for all chrome, with **Noto Color Emoji** as fallback (advanced shaping) so emoji never render as monochrome dingbats. Apple: SF Pro; Android: Roboto — but keep the sizes above.
- A user text-size preference scales title/body/meta together; meta never drops below 10px.
- Long single-line strings (names, previews, links) wrap by word *or* glyph and are ellipsised with “…”; chat-list previews are fitted to **two lines** in the space left after the unread badge.

## Spacing, radius and shape

| Token | Value |
|---|---|
| `space.1` … `space.6` | 4 / 8 / 12 / 16 / 24 / 32 px |
| `radius.sm` | 8 — menu items, emoji suggestions |
| `radius.md` | 10 — primary / secondary / danger buttons, composer field |
| `radius.lg` | 12 — action tiles, rounded text fields, link box, cards, menus |
| `radius.bubble` | 16 — message bubbles |
| `radius.pill` | 999 — round icon buttons, unread badge, avatars |

- Page padding: 16px sides (`pad`), 8–10px between header and content (`pad_tight`).
- **Settings-style lists** (contact page, group/channel page, app settings, preferences): rows have 6px vertical / 8px horizontal padding; lists of people (members, contacts to invite) sit **flush** (0 spacing, 2px row padding) with a 28px avatar before the name.
- Hit targets: ≥ 32×32 for icon buttons on desktop, ≥ 40–48 on mobile.

## Components

### Buttons — the complete set

Every button in Cursus is one of these. Pick by *role*, never by look.

| Kind | Linux style fn | Shape | Rest | Hover / press | Used for |
|---|---|---|---|---|---|
| **Primary** | `primary_action` | radius 10, padding 12×16 (8×12 small) | `accent` fill, `accent.on` text | `accent.hover` / `accent.press` | The one main action of a screen: Connect, Next, Accept, Create link |
| **Secondary** | `secondary_action` | radius 10, 1px `border` | `bg.sidebar` fill | `bg.hover` | Neutral alternatives: Cancel, menu-choice chips (`Short ▾`, `Member ▾`), header “+” |
| **Danger (solid)** | `danger_action` | radius 10 | `danger` fill, white text | darker red | Final confirmation of destruction (Leave / Delete confirm) |
| **Action tile (ghost)** | `ghost_tile` | radius 12, padding 8×10 | **no fill, no border** | `bg.hover` / `accent.soft` | Chat actions (Clear, Leave), Support (“Chat with admins”), Invite, Subscribers — icon + label |
| **Danger tile (ghost)** | `ghost_danger` | radius 12 | no fill, no border, red label + icon | `danger` @ 14% / 24% | Delete chat / group / channel / contact |
| **Icon-only tool** | `composer_hit` + `*_tool` icon | 32×32 hit, 20px glyph | glyph `text.secondary` | glyph → `text.primary` | Back, paste, favourite, mute, settings, role, member actions, add contact |
| **Round icon** | `round_icon` | pill | no fill | `bg.hover` / `accent.soft` | Floating round controls |
| **Menu item** | `menu_item` | radius 8, padding 10×12, full width | transparent | `bg.hover` / `accent.soft` | Rows inside overlay menus and context menus; disabled = `text.muted` label, no press |
| **Link box** | `link_box` | radius 12, 1px `accent` border | `accent.soft` fill, `accent` text, copy glyph at the end | `accent.soft.press` | A shareable link — the whole box copies |
| **Nav destination** | `nav_active` / `nav_idle` | radius 12 | active: `accent.soft` + accent glyph | `bg.hover` | The four destinations |
| **Chip** | `reaction_chip` | radius 14, 1px border | `bg.elevated` (selected: accent) | `accent.soft` | Reactions |

Placement rules: one primary per view; destructive actions are ghost-danger tiles that lead to a confirm step with a solid danger button; icon-only tools are always 32px hits with the same 20px glyph box so nothing looks bigger than the settings gear.

### Text fields

| Kind | Style fn | Shape | Notes |
|---|---|---|---|
| Standard field | `composer_input` | radius 10, 1px `border` → `accent` when focused, `bg.sidebar` fill, padding = `composer` | Forms, invite link, group description |
| Rounded inline field | `rounded_field` | radius 12 (= action tiles), padding 6×12, placeholder in `text.secondary` | Local name / alias under a profile name; side padding ≥ radius so corners never clip the placeholder |
| Composer | `composer_editor` | radius 10, no border | Message composer |

All editable fields (except passwords) offer the **edit menu**: right-click (desktop) or long-press (touch) → *Select all · Cut · Copy · Paste* backed by the system clipboard. Cut/Copy are disabled without a selection, Paste when the clipboard is empty, Select all when the field is empty. (Linux: `edit_menu::edit_field`, built on `iced_aw::ContextMenu`.) A field that accepts links has an icon-only **paste** tool beside it.

### Overlays and menus

- **Choice menu** (“nice overlay menu”): centred card (`bg.elevated`, radius 12, max 400px wide) over a 45% black backdrop; title in `text.muted`, options as menu items, the current choice in `accent`. Options that represent roles show the role glyph before the name. Tap outside or Esc closes.
- **Context menu**: small card at the pointer, radius 12, soft shadow, no backdrop.
- Confirmations live inside the same card (replace the actions with the question + Cancel / solid danger).

### Lists and rows

- Chat rows: 44px avatar, name (body), two-line preview (meta, `text.secondary`, ellipsised), trailing badges (favourite star, muted, unread pill in `accent` on `accent.soft`).
- Member rows: 28px avatar, name (yourself: **`YOU (Public name)`**), then a **gold** pending clock right after the name for invited / connecting members, then role and actions tools on the right.

## Icons

- One outlined family: 24×24 viewBox, **1.5px stroke**, round caps (Solar Linear style). New glyphs are drawn to that grid (Cursus originals are CC0).
- Always rendered as tinted vector (SVG) — never from symbol fonts. Tint follows the text token of the context (`text.secondary` idle, `text.primary` hover, `accent` active, `danger` destructive, `gold` pending).
- Glyph box 20px inside a 32px hit for tools; 16–18px inside menus and tiles.
- Third-party glyphs are listed with licence in `linux/assets/icons/ATTRIBUTION.md`; other platforms must ship the same files (or the same glyphs) and keep that attribution.

Destinations: Chats (bubble), Channels (antenna), Calls (handset), Settings (gear).

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

## Motion

- 120–200ms ease for nav selection and list → conversation push
- No continuous decorative animation
- Respect OS “reduce motion”

## Empty states

One short sentence + one primary action max. No mascot spam.

## Theming

Light and dark from day one on desktop. Follow OS preference by default; Settings may override later.

## Re-implementing the look on another platform

1. **Tokens first.** Copy the colour table into the platform's theme object (Compose `ColorScheme` + an extra `CursusColors` for `bg.hover`, `accent.soft`, `gold`; SwiftUI `Color` assets with light/dark variants). Keep names identical so code reviews can diff them against this file.
2. **Shapes.** Define four shape constants (8 / 10 / 12 / 16 + pill) and use them only through the component list above — never inline numbers.
3. **Build the button set once.** Implement each row of the button table as one reusable component (`PrimaryButton`, `SecondaryButton`, `ActionTile(ghost, danger)`, `IconTool`, `MenuItem`, `LinkBox`, …). Screens compose these; they do not style buttons themselves.
4. **Fields.** One standard field and one rounded inline field; both get the platform's native edit menu (Android/iOS already provide Select all / Cut / Copy / Paste on long-press — keep it enabled; desktop toolkits must add it).
5. **Icons.** Export the SVGs from `linux/assets/icons/` (Android: VectorDrawable; Apple: SF-Symbol-sized PDF/SVG templates) and tint with tokens.
6. **Typography.** Map title / body / meta per breakpoint; respect the OS text-size setting by scaling all three together.
7. **Check** each new screen against the checklist below and compare it side-by-side with the Linux build in both themes.

## Checklist for new UI work

1. Does it preserve the four destinations?
2. Does it use tokens from this document?
3. Are breakpoints logical-px only?
4. Is navigation native to the platform?
5. Would the screen still feel like Cursus if the OS chrome were swapped?
6. Is every button one of the kinds in the button table (and only one primary)?
7. Do editable fields offer Select all / Cut / Copy / Paste?
