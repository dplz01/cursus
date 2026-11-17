# Cursus design system

**Canonical reference** for Linux, macOS, Android, and iOS.

> One product, one design language, one shared Rust brain, four native experiences.

Each platform implements these tokens and structures in its **native** UI toolkit. Native conventions win when they conflict with pixel-perfect matching. Do not ship a web shell inside four OS wrappers.

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

The same four destinations, the same primary content models (conversation list → conversation, etc.), and the same two responsive *layouts* (phone / desktop).

Let each platform render **its own native navigation**:

| Platform | Desktop / wide | Phone |
|---|---|---|
| Linux (Iced) | Vertical nav rail | Bottom destination bar |
| macOS (SwiftUI) | `NavigationSplitView` / sidebar | Tab bar / stack |
| Android (Compose) | `NavigationRail` | `NavigationBar` |
| iOS (SwiftUI) | Sidebar / split | `TabView` + navigation stack |

That keeps Cursus recognizable without making it feel like one web app pasted onto four OSes. Shared Rust (`cursus-core` + `libsimplex`) owns session, commands, and models — not pixels.

## Navigation and back

- Show the name under each destination icon in the rail and bottom bar. Keep all four destinations equally easy to find.
- Header Back, Escape and Android system Back go up one visible layer: sheet → nested page → profile → conversation → list. A root list or locked screen has no parent.
- Switching destinations dismisses the previous destination's pages and sheets. Chat profiles, group preferences and compose pages render only inside Chats or Channels. Settings and Calls own their detail panes.
- Leaving a profile commits its existing drafts, just as its header Back does. Escape inside a text editor dismisses that unsaved edit first.
- Desktop Settings has an explicit Back control. Closing a category leaves the category list and a quiet prompt in the detail pane.
- Empty detail panes use a centred icon, heading and short sentence, with at most one primary action. Chats and Channels offer a new conversation; Calls points to contacts; Settings points to categories.

## Color

Exact values live in `linux/src/theme.rs` (`Tokens::light()` / `Tokens::dark()`); this table is the source of truth for every other platform.

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg.app` | `#EEF0F4` | `#0E1015` | Window / root background |
| `bg.sidebar` | `#FFFFFF` | `#15181E` | Nav rail and sidebar |
| `bg.list` | `#FFFFFF` | `#15181E` | Middle column / lists |
| `bg.panel` | `#EEF0F4` | `#0E1015` | Conversation pane, settings pages |
| `bg.elevated` | `#FFFFFF` | `#1C2028` | Menus, popups, incoming bubbles |
| `bg.hover` | `#EDEFF3` | `#20242D` | Hover fill for every quiet control |
| `bg.field` | `#F2F4F7` | `#111318` | Text fields and quiet buttons |
| `bubble.in` / `bubble.out` | `#FFFFFF` / `#D8E5FF` | `#1C2028` / `#243A63` | Incoming / outgoing messages |
| `bubble.out.meta` | `#5A6C8F` | `#A7B6D6` | Time and secondary text on outgoing messages |
| `text.primary` | `#12141A` | `#F2F3F5` | Body / titles |
| `text.secondary` | `#5C6370` | `#A0A6B3` | Previews, idle icons, placeholders in rounded fields |
| `text.muted` | `#8B919C` | `#6E7585` | Section labels, hints, disabled labels |
| `accent` | `#2F6FED` | `#5B8CFF` | Primary actions, selection, links |
| `accent.hover` / `accent.press` | `#255DD1` / `#1C4CB8` | `#6B99FF` / `#4D7AEB` | Primary button states |
| `accent.on` | `#FFFFFF` | `#0B1020` | Text / icons on accent |
| `accent.soft` | accent @ 12% | accent @ 16% | Selected rows, unread badge, link box |
| `accent.soft.press` | accent @ 30% | accent @ 36% | Pressed soft surfaces |
| `border` | `#DDE1E7` | `#252A33` | Hairlines (1px) |
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

| Role | Desktop | Phone | Weight |
|---|---|---|---|
| App / page title | 18px | 17px | 600 |
| Profile name | title + 4 | title + 4 | 600 |
| Section label | = meta | = meta | 600, `accent` |
| Chat name / body / message | 14px | 16px | 400 |
| Preview / meta / buttons | 12px | 13px | 400 |

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
- **Settings-style lists** (contact page, group/channel page, app settings, preferences): rows have 6px vertical / 8px horizontal padding; lists of people (members, contacts to invite) sit **flush** (0 spacing, 4px row padding) with a 40px avatar before the name.
- Hit targets: ≥ 32×32 for icon buttons on desktop, ≥ 40–48 on mobile.

## Components

### Buttons — the complete set

Every button in Cursus is one of these. Pick by *role*, never by look.

| Kind | Linux style fn | Shape | Rest | Hover / press | Used for |
|---|---|---|---|---|---|
| **Primary** | `primary_action` | radius 10, padding 12×16 (8×12 small) | `accent` fill, `accent.on` text | `accent.hover` / `accent.press` | The one main action of a screen: Connect, Next, Accept, Create link |
| **Secondary** | `secondary_action` | radius 10, 1px `border` | `bg.sidebar` fill | `bg.hover` | Neutral alternatives: Cancel, menu-choice chips (`Short ▾`, `Member ▾`), header “+” |
| **Danger (solid)** | `danger_action` | radius 10 | `danger` fill, white text | darker red | Final confirmation of destruction (Leave / Delete confirm) |
| **Action tile (ghost)** | `ghost_tile` | radius 12, padding 10×8 | **no fill, no border**; 32px `accent.soft` icon disc + body-size label (same sizes as navigation rows) | `bg.hover` / `accent.soft` | Chat actions (Clear, Leave) |
| **Danger tile (ghost)** | `ghost_danger` | radius 12 | no fill, no border; 32px red-tinted disc (`danger` @ 14%), red body label | `danger` @ 14% / 24% | Delete chat / group / channel / contact |
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

All editable fields (except passwords) **and the chat composer** offer the **edit menu**: right-click (desktop) or long-press (touch) → *Select all · Cut · Copy · Paste* backed by the system clipboard. A long-press keeps an existing selection (the finger-down would otherwise clear it). Cut/Copy are disabled without a selection, Paste when the clipboard is empty, Select all when the field is empty. (Linux: `edit_menu::edit_field`, built on `iced_aw::ContextMenu`.) A field that accepts links has an icon-only **paste** tool beside it.

### Overlays and menus

- **Choice menu** (“nice overlay menu”): centred card (`bg.elevated`, radius 12, max 400px wide) over a 45% black backdrop; title in `text.muted`, options as menu items, the current choice in `accent`. Options that represent roles show the role glyph before the name. Tap outside or Esc closes.
- **Context menu**: small card at the pointer, radius 12, soft shadow, no backdrop.
- Confirmations live inside the same card (replace the actions with the question + Cancel / solid danger).

### Profile header (one component everywhere)

Contact, group, channel and own-profile pages open with the same centred header (Linux `profile_identity`, Android `ProfileIdentity`):

- Picture, 96, centred; tapping it changes it where that is allowed.
- Name: title size + 4, bold, centred. Editable lines carry a small pencil right after the text.
- Under it, centred, `text.secondary`: local name, then the bio / short description.
- **Quick actions** (Linux `quick_actions`, Android `QuickActions`): a centred row of 52×44 rounded squares (`bg.elevated`, radius 12) with a 22px glyph and a meta label below — Call, Star, Mute, Preferences. The glyph turns accent when the state is on. These replace icons in the title bar; the title bar only has Back and the page name.
- Longer texts (About, Welcome message) follow as a section: label + paragraph at the row inset.

### Preference panels

Settings categories and contact / group / channel preferences use the same mobile-first inset panels (Android `PreferenceGroup` / `PreferenceRow`, Linux `preference_ui::group` / `preference_row_content`, macOS `PreferenceGroup` / `PrefRow`).

- Page width: at most 620, centred, with 16px side insets. Groups are 20px apart.
- Group: `bg.elevated`, 20px corners, quiet `border` at 50% opacity, no shadow. A small secondary heading sits outside the panel, 16px inset and 9px above it. Optional scope / ownership notes sit below it at the same inset.
- Rows: 16px horizontal and 13–14px vertical padding; 14px between icon, flexible label column and fixed control column. Titles use body size; a short secondary explanation appears 4px below. Text wraps freely, preserving the switch's width even on a narrow phone.
- Optional leading glyph: 18px in a 32px `accent.soft` square with 10px corners. App privacy rows omit it; group feature rows include it.
- Switch: 46×28, 20px white thumb, 4px inset. On: `accent`; off: `text.secondary` at 24%. Disabled controls fade to 45%. Android and macOS animate state changes briefly. The entire switch row toggles once; its help button remains a separate action.
- Longer explanations and existing confirmations remain available. Inline text answers the everyday question; it does not replace privacy warnings.
- Group / channel sections: **Messages**, **Media & interaction**, **Members**, then **Just for you** where applicable. Owners see a note that changes affect everyone; other members see an ownership note and disabled controls. Review new members belongs to groups; channels do not offer it.

### Pages: flat sections, one row (the Cursus look)

Profile overviews, the Settings category list, Invite and New keep the same flat rows as the chat list. Preference pages use the inset panels above, giving related controls a shared surface. (Until 2026-11 these pages were stacks of cards; the component names `card` / `CursusCard` / `Card` remain in code and now mean "section".)

- **Page column**: at most 620 wide, centred; 16 side padding; 22 between sections.
- **Section label**: meta size, bold, `accent`, inset 12, 4 above its rows. No icon.
- **Row** — navigation and actions on flat pages:
  - inset 12 left and right, 10 above and below; radius 12 hover / press tint (`ghost tile`);
  - leading glyph 20 in a 24 box, **no disc behind it**; 14 gap to the label;
  - label in body size; optional one-line meta subtitle;
  - trailing: chevron, switch, value chip or count.
- **Switch**: use the shared 46×28 preference switch described above. Tapping the setting's name toggles it too; ⓘ stays its own button.
- **Destructive actions** (clear / leave / delete) are ordinary rows in red at the bottom of the page, never tiles.
- Everything on a page starts at the same inset: labels, rows, paragraphs, link boxes.

### Settings layout

Settings are a list of categories, each opening its own page (as on Android): beside the list when there are two columns, over it (with Back) when there is one. The list's header is your profile (picture, name, bio) and opens the profile page. Each row is an icon and a title — no subtitle.

| Page | Contains |
|---|---|
| Profile | picture, name, bio, description |
| Your address | create / show / hide, share on profile, auto-accept, delete |
| Privacy | on screen (previews, blur, screenshots on Android) · what contacts see (delivery receipts) · links (remove tracking) |
| Security | app lock, lock after, lock now · unlocking at start |
| Files and network | downloads (automatic download, protect IP) · on this device (encrypt stored files) |
| Notifications | alerts, hide message text · background (tray on Linux, background service on Android) |
| Appearance | theme, text size · typing (Linux) |
| About | version, source code, licences |

### Panes

Desktop panes are separated by a single 1px `border` hairline between the list and the main pane — no boxed borders around the rail or list.

### Lists and rows

- Names and previews never wrap past their budget: one line for names / bios in member lists, two lines for chat previews, ending in “…”. The fit happens at layout time with the real width (Linux: `ellipsis::ellipsis_text`).
- Chat rows: 44px avatar in a fixed slot (with or without the group badge, so names line up), name (body), two-line preview (meta, `text.secondary`); text with long unbroken runs (links, keys) wraps by glyph so both lines fill. Trailing badges: favourite star, muted, unread pill in `accent` on `accent.soft`.
- Member / contact rows: 40px avatar, name (yourself: **`YOU (Public name)`**) with the short bio under it, a fixed 16px **gold** pending clock right after invited / connecting names (its space is reserved first, so it never shrinks away), then role and actions tools on the right. Rows sit flush (4px vertical padding).

## Icons

- One outlined family: 24×24 viewBox, **1.5px stroke**, round caps (Solar Linear style). New glyphs are drawn to that grid (Cursus originals are CC0).
- Always rendered as tinted vector (SVG) — never from symbol fonts. Tint follows the text token of the context (`text.secondary` idle, `text.primary` hover, `accent` active, `danger` destructive, `gold` pending).
- Glyph box 20px inside a 32px hit for tools; 16–18px inside menus and tiles.
- Third-party glyphs are listed with licence in `linux/assets/icons/ATTRIBUTION.md`; other platforms must ship the same files (or the same glyphs) and keep that attribution.

Destinations: Chats (bubble), Channels (antenna), Calls (handset), Settings (gear).

## Layout (logical pixels)

Sizes are **logical / density-independent pixels**, never framebuffer pixels.

The window's width alone decides how many columns fit — the Telegram Desktop model (Linux `layout.rs`):

| Width | Columns |
|---|---|
| under 600 | **one**: the list, or the open chat / page over it; destinations in a floating bar at the bottom |
| from 600 | **two**: list + chat; the destinations bar sits under the list |
| from 900 | the destinations move to a **rail** on the left |
| chat would keep ≥ 380 next to a 340 info column (about 1130 with the rail) | **three**: list + chat + info — an open contact / group page sits beside its chat instead of replacing it |

- The list is 27% of the width, between 260 and 340.
- One column uses touch sizes (body 16, bar 54); two and three columns use desktop sizes (body 14).
- Pages (profile, settings) are at most 620 wide, centred; a wide one-column list at most 720.
- Everything must still work at 280 wide: rows wrap their labels, the four quick actions share the width equally.

### Desktop structure

1. **Nav** — four destinations (platform-native rail / sidebar)
2. **List** — conversations / channels / calls / settings groups (~280–320 logical px)
3. **Main** — selected conversation or empty state

### Phone structure

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

### Navigation clicks

Back and other page-changing header controls complete an owned press-and-release before navigating. A release arriving after a view changes must never activate the newly exposed conversation header. Composer controls may still act on press to keep the on-screen keyboard usable.
