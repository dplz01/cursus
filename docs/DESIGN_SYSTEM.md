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
| macOS (SwiftUI) | Native split with floating bottom destination dock below list | — |
| Android (Compose) | Floating destination dock below list | Floating bottom destination dock |
| iOS (SwiftUI) | Sidebar / split | `TabView` + navigation stack |

Android uses one pane below 600dp of available app-window width, and two panes above it. The two-pane shell always keeps its destination dock below the list, with every settings category and chat detail in the right pane; it never adds a rail. Split decisions use actual window constraints, including floating and split-screen windows.

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
| `bg.elevated` | `#FFFFFF` | `#252B36` | Menus, popups, panels |
| `bg.hover` | `#EDEFF3` | `#303746` | Hover fill for every quiet control |
| `bg.field` | `#F2F4F7` | `#111318` | Text fields and quiet buttons |
| `bubble.in` / `bubble.out` | `#FFFFFF` / `#D8E5FF` | `#1C2028` / `#243A63` | Incoming / outgoing messages |
| `bubble.out.meta` | `#5A6C8F` | `#A7B6D6` | Time and secondary text on outgoing messages |
| `text.primary` | `#12141A` | `#F2F3F5` | Body / titles |
| `text.secondary` | `#5C6370` | `#A0A6B3` | Previews, idle icons, placeholders in rounded fields |
| `text.muted` | `#656D79` | `#8B93A3` | Section labels, hints, disabled labels |
| `accent` | `#2F6FED` | `#5B8CFF` | Primary actions, selection, links |
| `accent.hover` / `accent.press` | `#255DD1` / `#1C4CB8` | `#6B99FF` / `#4D7AEB` | Primary button states |
| `accent.on` | `#FFFFFF` | `#0B1020` | Text / icons on accent |
| `accent.soft` | accent @ 12% | accent @ 16% | Selected rows, unread badge, link box |
| `accent.soft.press` | accent @ 30% | accent @ 36% | Pressed soft surfaces |
| `border` | `#DDE1E7` | `#3B4453` | Hairlines (1px) |
| `danger` | `#C62828` | `#FF6B6B` | Destructive labels / icons |
| `success` | `#2E7D32` | `#6BCB77` | Sent / connected / accept |
| `gold` (`always`) | `#B58B0F` | `#ECC14F` | Pending / invited status, asymmetric “always” prefs, support badge |
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
| Section label | = meta | = meta | 600, `text.secondary` |
| Chat name / body / message | 14px | 16px | 400 |
| Preview / meta / buttons | 12px | 13px | 400 |

- Font: **Noto Sans** (Regular / Bold / Italic) for all chrome, with **Noto Color Emoji** as fallback (advanced shaping) so emoji never render as monochrome dingbats. Apple: SF Pro; Android: Roboto — but keep the sizes above.
- A user text-size preference scales title/body/meta together; meta never drops below 10px.
- Long single-line strings (names, previews, links) wrap by word *or* glyph and are ellipsised with “…”; chat-list previews are fitted to **two lines** in the space left after the unread badge.

## Spacing, radius and shape

| Token | Value |
|---|---|
| `space.1` … `space.6` | 4 / 8 / 12 / 16 / 24 / 32 px |
| `radius.sm` | 20 — menu items |
| `radius.md` | 20 — primary / secondary / danger buttons, form fields |
| `radius.lg` | 20 — action tiles, rounded text fields, link box, cards, menus |
| `radius.bubble` | 16 — message bubbles |
| `radius.pill` | 999 — round icon buttons, unread badge, avatars |
| `radius.inset` | 16 — rows inside a panel (concentric with its 20 corner and 4 inset) |
| `radius.small` | 12 — compact square controls: header actions, voice play |

- Page padding: 16px sides (`pad`), 8–10px between header and content (`pad_tight`).
- **Settings-style lists** (contact page, group/channel page, app settings, preferences): rows have 6px vertical / 8px horizontal padding; lists of people (members, contacts to invite) sit **flush** (0 spacing, 4px row padding) with a 40px avatar before the name.
- Hit targets: ≥ 32×32 for icon buttons on desktop, ≥ 40–48 on mobile.

## Components

### Buttons — the complete set

Every button in Cursus is one of these. Pick by *role*, never by look.

| Kind | Linux style fn | Shape | Rest | Hover / press | Used for |
|---|---|---|---|---|---|
| **Primary** | `primary_action` | radius 20, padding 12×16 (8×12 small) | `accent` fill, `accent.on` text | `accent.hover` / `accent.press` | The one main action of a screen: Connect, Next, Accept, Create link |
| **Secondary** | `secondary_action` | radius 20, 1px `border` | `bg.sidebar` fill | `bg.hover` | Neutral alternatives: Cancel, Replace link |
| **Header action** | `soft_action` | 36px, radius 12 (`radius.small`) | `accent.soft` fill, accent glyph, no border | `accent.soft` hover / press | “+” on Chats / Channels, phone on Calls (macOS 26: native glass) |
| **Choice value** | `choice_chip` / `row_value` | radius 12 tint on hover | current value in `text.secondary` + chevron, no box | `bg.hover` | A setting with several values; on macOS a native borderless menu |
| **Danger (solid)** | `danger_action` | radius 20 | `danger` fill, white text | darker red | Final confirmation of destruction (Leave / Delete confirm) |
| **Action tile (ghost)** | `ghost_tile` | radius 20, padding 10×8 | **no fill, no border**; 32px `accent.soft` icon disc + body-size label (same sizes as navigation rows) | `bg.hover` / `accent.soft` | Chat actions (Clear, Leave) |
| **Danger tile (ghost)** | `ghost_danger` | radius 20 | no fill, no border; 32px red-tinted disc (`danger` @ 14%), red body label | `danger` @ 14% / 24% | Delete chat / group / channel / contact |
| **Icon-only tool** | `composer_hit` + `*_tool` icon | 32×32 hit, 20px glyph | glyph `text.secondary` | glyph → `text.primary` | Back, paste, favourite, mute, settings, role, member actions, add contact |
| **Round icon** | `round_icon` | pill | no fill | `bg.hover` / `accent.soft` | Floating round controls |
| **Menu item** | `menu_item` | radius 20, padding 10×12, full width | transparent | `bg.hover` / `accent.soft` | Rows inside overlay menus and context menus; disabled = `text.muted` label, no press |
| **Link box** | `link_box` | radius 20, 1px `accent` border | `accent.soft` fill, `accent` text, copy glyph at the end | `accent.soft.press` | A shareable link — the whole box copies |
| **Nav destination** | `nav_active` / `nav_idle` | radius 20 | active: `accent.soft` + accent glyph | `bg.hover` | The four destinations |
| **Chip** | `reaction_chip` | radius 20, 1px border | `bg.elevated` (selected: accent) | `accent.soft` | Reactions |

Placement rules: one primary per view; destructive actions are ghost-danger tiles that lead to a confirm step with a solid danger button; icon-only tools are always 32px hits with the same 20px glyph box so nothing looks bigger than the settings gear.

### Text fields

| Kind | Style fn | Shape | Notes |
|---|---|---|---|
| Standard field | `composer_input` | radius 20, 1px `border` → `accent` when focused, `bg.sidebar` fill, padding = `composer` | Forms, invite link, group description |
| Rounded inline field | `rounded_field` | radius 20 (= action tiles), padding 6×12, placeholder in `text.secondary` | Local name / alias under a profile name; side padding ≥ radius so corners never clip the placeholder |
| Composer | `composer_editor` | radius 20, no border | Message composer |

All editable fields (except passwords) **and the chat composer** offer the **edit menu**: right-click (desktop) or long-press (touch) → *Select all · Cut · Copy · Paste* backed by the system clipboard. A long-press keeps an existing selection (the finger-down would otherwise clear it). Cut/Copy are disabled without a selection, Paste when the clipboard is empty, Select all when the field is empty. (Linux: `edit_menu::edit_field`, built on `iced_aw::ContextMenu`.) A field that accepts links has an icon-only **paste** tool beside it.

### Overlays and menus

- **Choice menu** (“nice overlay menu”): centred card (`bg.elevated`, radius 20, max 400px wide) over a 45% black backdrop; title in `text.muted`, options as menu items, the current choice in `accent`. Options that represent roles show the role glyph before the name. Tap outside or Esc closes.
- **Context menu**: small card at the pointer, radius 20, soft shadow, no backdrop.
- Confirmations live inside the same card (replace the actions with the question + Cancel / solid danger).

### Profile header (one component everywhere)

Contact, group, channel and own-profile pages open with the same centred header (Linux `profile_identity`, Android `ProfileIdentity`):

- Picture, 96, centred; tapping it changes it where that is allowed.
- Name: title size + 4, bold, centred. Editable lines carry a small pencil right after the text.
- Under it, centred, `text.secondary`: local name, then the bio / short description.
- **Quick actions** (Linux `quick_actions`, Android `QuickActions`): a centred row of complete rounded buttons (`bg.elevated`, 1px border, radius 16). Each contains a 24px glyph and its meta label, with 12px vertical padding and an 8px gap between buttons — Call, Star, Mute, Settings. The entire button, including its label and padding, illuminates the glyph on hover. The glyph also turns accent when the state is on. These replace icons in the title bar; the title bar only has Back and the page name.
- Longer texts (About, Welcome message) follow as a section: label + paragraph at the row inset.

### Preference panels

Settings categories and contact / group / channel preferences use the same mobile-first inset panels (Android `PreferenceGroup` / `PreferenceRow`, Linux `preference_ui::group` / `preference_row_content`, macOS `PreferenceGroup` / `PrefRow`).

- Page width: at most 620, centred, with 16px side insets. Groups are 20px apart.
- Group: `bg.elevated`, 20px corners, quiet `border` at 50% opacity, no shadow. A small secondary heading sits outside the panel, 16px inset and 9px above it. Optional scope / ownership notes sit below it at the same inset.
- Rows: 16px horizontal insets (12px below 360px), with a title/control line and full-width supporting text underneath. Help is attached to the supporting text instead of taking space from the title. Below 360px, decorative icons are omitted; text gets the available width while switches keep their size. Text wraps naturally; it is never squeezed into a tall column beside several controls.
- Optional leading glyph: 18px in a 32px `accent.soft` square with 10px corners. App privacy rows omit it; group feature rows include it.
- Switch: 46×28, 20px white thumb, 4px inset. On: `accent`; off: `text.secondary` at 24%. Disabled controls fade to 45%. Android and macOS animate state changes briefly. The entire switch row toggles once; its help button remains a separate action.
- Longer explanations and existing confirmations remain available. Inline text answers the everyday question; it does not replace privacy warnings.
- Group / channel sections: **Messages**, **Media & interaction**, **Members**, then **Just for you** where applicable. Owners see a note that changes affect everyone; other members see an ownership note and disabled controls. Review new members belongs to groups; channels do not offer it.

### Pages: panels (the Cursus look)

Since 2026-11-25 every list of settings or actions sits in a **panel** — the
same floating surface on every page, so Settings, profiles and preferences look
alike wherever they appear. Only the profile hero (picture, name, bio, quick
actions), paragraphs and the conversation / call lists stay flat.

- **Page column**: at most 620 wide, centred; 16 side padding; 22 between sections.
- **Panel** (Linux `preference_ui::panel`, Android `Panel`, macOS `Panel` /
  `Card(padding: 0)`): `bg.elevated`, 1px `border`, radius 20, 4 inset, no shadow.
- **Section heading** (Linux `preference_ui::heading`, Android `SectionHeading`,
  macOS `SectionHeading`): meta size, 600, `text.secondary`, *outside* the panel,
  inset 20 (= panel 4 + row 16) so it lines up with the row text; 8 above the
  panel. Notes use the same inset in regular weight. A page never repeats its own
  title as a heading.
- **Row** inside a panel: 16 horizontal inset, radius 16 hover / press tint;
  glyph 20 in a 24 slot, accent, **no disc**, 14 gap; label in body size;
  optional one-line subtitle. Phone rows: 56 high for one line, 68 with a subtitle
  (Android keeps its 64dp minimum).
- **Trailing**: a chevron means “opens a page or menu”. Rows that open a choice
  show the current value before the chevron and the whole row is the button.
  Immediate actions (Hide messages, Block, Lock now) have no chevron.
- **Destructive actions** (leave / delete / remove) are red rows in their own
  panel at the bottom; Clear chat stays neutral.

### Settings layout

Settings are a list of categories, each opening its own page (as on Android): beside the list when there are two columns, over it (with Back) when there is one. The list's header is your profile (picture, name, bio) and opens the profile page. Each row is an icon, a title and a chevron, grouped in three panels: Profile · Address / Privacy & Security · Notifications · Appearance / About. The Settings list sits on `bg.panel` like its pages. macOS keeps its native sidebar rows here (System Settings convention); its pages use the panels.

| Page | Contains |
|---|---|
| Profile | picture, name, bio, description |
| Address | create / show / hide, share on profile, auto-accept, delete |
| Privacy & Security | app lock and unlocking at start · on screen (previews, blur, screenshots on Android) · delivery receipts · links · downloads and IP protection · stored-file encryption |
| Notifications | alerts, hide message text · background (tray on Linux and macOS, background service on Android) |
| Appearance | theme, text size · typing (Linux) |
| About | version, source code, licences |

### Panes

Desktop panes are separated by a single 1px `border` hairline between the list and the main pane — no boxed borders around the rail or list.

### Lists and rows

- Names and previews never wrap past their budget: one line for names / bios in member lists, two lines for chat previews, ending in “…”. The fit happens at layout time with the real width (Linux: `ellipsis::ellipsis_text`).
- Chat rows: 44px avatar in a fixed slot (with or without the group badge, so names line up), name (body), two-line preview (meta, `text.secondary`); text with long unbroken runs (links, keys) wraps by glyph so both lines fill. Trailing badges: favourite star, muted, unread pill in solid `accent` with `accent.on` text (muted chats: `text.muted`).
- **Avatars without a picture**: up to two initials (first letters of the first two words) in `accent`, semibold, on an `accent.soft` disc, no outline; groups and channels show their glyph in `accent` on the same disc. One rule for contacts, members and yourself on every platform.
- Member / contact rows: 40px avatar, name (yourself: **`YOU (Public name)`**) with the short bio under it, a fixed 16px **gold** pending clock right after invited / connecting names (its space is reserved first, so it never shrinks away), then role and actions tools on the right. Rows sit flush (4px vertical padding).

## Icons

- One outlined family: 24×24 viewBox, **1.5px stroke**, round caps (Solar Linear style). New glyphs are drawn to that grid (Cursus originals are CC0). Filled glyphs only express state (starred, muted); navigation rows use `notifications-outline`, `flag-outline`, `chevron-right`.
- Never rely on rotating an SVG at runtime (some renderers skip it): ship the turned glyph (`chevron-right.svg`, `next.svg`).
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

### Touch menus and modal state

- A stationary long press opens the same menu as desktop right-click: on a message, the bubble menu; on a conversation preview, the chat menu. The touched widget owns the timer; no prior hover event is required. Swipes, cancelled touches and the lift after a hold do not activate the underlying content.
- Opening a modal keeps the background widget tree and scroll position intact. Background mouse, touch and keyboard input is suspended until dismissal; an opening member menu cancels any inertial fling.
- Layout reference: [Signal Android shared rows](https://github.com/signalapp/Signal-Android/blob/main/core/ui/src/main/java/org/signal/core/ui/compose/Rows.kt) uses a weighted text column and a dedicated switch; [Signal Desktop preferences](https://github.com/signalapp/Signal-Desktop/blob/main/stylesheets/components/Preferences.scss) reflows narrow rows at a container breakpoint. Cursus uses full-width supporting text and removes decorative icons when narrow.

- Android header Back uses the AndroidX back dispatcher, shared with system Back and keyboard Escape; all chat/profile headers keep the same 56dp height and leading inset.
- Linux supports both native touch holds and mouse-emulated holds. Icon hits glow across the full target without hover boxes; the bottom dock keeps its existing hover style. Hover fills inside grouped cards stay inset from their borders, with visible contrast in both themes.
- Pasting an image into the composer creates a removable draft preview; Send submits it. Text remains available as a separate draft. Linux supports Ctrl+V, Shift+Insert and the edit menu; Android uses the native rich-content receiver for clipboard and keyboard images.
- Tapping a downloaded image opens an in-app viewer on Android and Linux: Back/Escape closes it, pinch or wheel zooms, and swiping changes images while at normal zoom. Zoomed images pan within the viewer.

- Chat settings use compact grouped surfaces: Notifications for receipts, Chat permissions for negotiated features. Rows without supporting text use 8px vertical padding; longer descriptions retain a full-width support line.
- The Invite page separates “Share an invite” from “Invite contacts”. The link is a neutral two-line preview with one full-width Copy action, secondary format/role controls and a quiet destructive Revoke action. The contact picker has search and complete 56px rows.
- The call list uses the same Jtb phone family on both platforms: green incoming, blue outgoing, red missed/declined/failed. Each history record opens its chat and has a separate call-back action.
- Linux scrollables translate finger coordinates at their content boundary, including end-anchored history. Any viewport travel cancels a pending hold. Live dragging stays native; inertia is widget-local and uses elapsed-time exponential deceleration, with no app rebuild on each animation frame.
- Linux media uses Iced's native image viewer for zoom/pan. Wheel zoom anchors at the cursor; pinch zoom and translation follow the finger midpoint. Swiping changes photos only at fit scale, and neither arrows nor swipes wrap at the first/last photo. Names are ellipsized in the header without changing file paths.

- Touch input suppresses all Linux hover styling from press through lift. Cursor positions remain available for hit testing; only genuine mouse movement restores hover. Styles read the live input mode so views built before the touch cannot retain a glow.
- Dark grouped surfaces share a brighter elevated token and a visible solid border on Android and Linux. Compact receipt rows have one 48dp minimum hit target and no duplicate copy beneath their group.
- Gallery titles use message attachment names, never decrypted cache basenames. Gallery decode runs in workers at Android's 2048px preview limit, with at most two workers and three neighbouring images resident. The covered chat page retains its widget tree but skips construction, layout and rendering until Back restores it.
- Call symbols use the existing Solar handset and matching rounded monoline status marks in a 24×24 viewBox. Status marks scale with the secondary text; names are the primary line and dates are quieter.
- Sliding gallery animation is postponed: Iced 0.14 exposes the native zoom/pan viewer, but no native paged-gallery transition. A custom pager/physics layer would need separate work. API checked: https://docs.rs/iced/0.14.0/iced/widget/index.html .

- Cancelled touch drags clear both the gesture wrapper and its pressed child control before consuming the lift. A subsequent tap must work without navigating or rebuilding the page. New kinetic presses start a session rather than toggling stale state. Deferred touch presses use their finger position for hit testing when redraws have no hover cursor.
- Group/channel profiles, settings and invite panes share the same 56px header and Back-button position. Missed calls use the shared Solar handset rotated down, with a returning arrow above it; declined calls use the cross-marked handset on both platforms.

- Touch cancellation is measured before scroll translation. The scrollable receives a cancelled release after any drag, including slow swipes ending on the starting control; its outer kinetic tracker still receives the physical lift. Deferred controls also cancel when the viewport moves.
- Calls retain their loaded history across navigation and invalidate it on call/clear/delete events. The first cold-load page is published immediately; refreshing keeps existing rows until the scan finishes. A contact-id index replaces per-record chat-list searches on both platforms. The shared scanner respects its exact item budget.

- Profile field editors share SimpleX markdown parsing and a formatting preview on Android and Linux. Source markup and literal shortcodes are preserved; profile fields do not autocomplete or expand emoji shortcodes. Profile names, aliases, bios and descriptions render through that same parser with a bounded cache.
- Message reactions display all eight supported emojis directly, wrapping on narrow screens without an additional picker. The bundled SimpleX core permits eight emoji reactions (`MREmojiChar`); arbitrary emoji cannot be sent as reactions without a protocol-core change. The UI does not substitute quoted messages or pretend unsupported reactions succeeded.
- Multi-state preferences show their value as quiet text with a chevron (see Pages); the choice menus are unchanged.

- SimpleX naming validation still applies to public names: the first character must be a letter, number or supported emoji, and some punctuation is prohibited. The editor explains this and offers inline bold/italic formatting after the first word; descriptions and local aliases are not restricted by those public-name rules.

- Group/channel short descriptions use the same multiline editor, sizing, scrolling and rich-text preview as welcome messages. Existing short-description length limits remain in force.

## macOS chrome

Mac navigation uses the shared Cursus colors and glyphs inside a floating
rounded dock over the bottom of the list in a native resizable list/detail split.
The bar has 20pt corners; selection, hover and hit shapes use 15pt
corners, matching the Linux and Android navigation bars.
The list viewport extends behind the dock. Chats, calls and settings add 84pt
of scroll-content padding so the last row can scroll completely above it.
The dock is a stable sibling of the changing page so all destination pairs
retain their animation state. One persistent selection lens moves between the
four destinations with an explicit spring-animated position; it is not replaced
or crossfaded per tab.
Pointer hover retains the ordinary Cursus hover fill and does not move the lens.
The dock floats exactly like the composer field on every platform: the same
glass surface (`FloatingSurface` on macOS, Linux `glass_fill`/`glass_edge`/
`glass_shadow`, Android `glass`). Linux and Android put the 8 px fade strip under
it too, fading into the list's own colour (`glass_scrim_on(bg.list)`): a fade
into another colour reads as a band and the bar looks docked. macOS has no
fade there, since the sidebar is the split view's own material.
The selection lens uses native clear Liquid Glass on macOS 26 and stays below
the controls so its material cannot blur icons or text.
A small animated glyph displacement peaks as a moving lens edge crosses each
tab and returns exactly to the unmodified rendering at rest. Hover does not
trigger that displacement.
Reduce Motion disables sliding. Reduce Transparency, Increase Contrast and
older systems use a bordered accent selection below the controls. macOS uses
NSSplitViewController with a native sidebar item: macOS 26 supplies the floating
glass surface and older systems supply their standard sidebar. Sidebar content
has transparent backgrounds so it does not obscure the system material. AppKit
owns the sidebar safe area, window-control region and resizing. Custom Mac
buttons share the 12pt control corner token; native window corners remain
system-managed. Intel and Apple Silicon follow identical availability rules.
Native macOS switches retain the Cursus accent and grouped-row geometry.

The optional macOS menu-bar item is controlled from Notifications → Menu bar.
It offers Open Cursus, Settings, Lock and Quit. Changing its visibility takes
effect immediately and persists across launches; closing the main window keeps
the existing background-receiving behavior, with or without the item.

Mac call history uses the same Solar incoming, outgoing, missed, declined and
active-call glyphs as Linux and Android. Missed/declined/failed calls are red,
outgoing calls use the accent and incoming calls use the success color. Call
controls also include action icons without changing their existing behavior.

### Creation, calls and group actions

Chats, Channels and Calls use the same rounded square + header action (10pt
corners). On Mac, this action and the three creation choices use native Liquid
Glass where available. Linux and Android use the corresponding opaque Cursus
field surface and border. Creation opens an overlay with Connect, New group
and New channel choices and in-place forms, preserving the page underneath.
Calls show history with a call-back action on every available contact. The +
action opens a searchable contact picker; missed names and statuses are red.

The selected Mac navigation tab has no hover fill over its glass lens.
Mac conversation views stay mounted beneath contact/group profiles to preserve
scroll position; Android saves the list state during profile visits. Returning
from a profile does not re-follow an existing outgoing message. Newly opened
conversations start at the last read message, before the unread boundary.

Group profile actions are ordered Welcome message, Chats with members / Chat
with admins, Share this group. Invite a contact sits under Members.
Member pages distinguish Direct message from Chat with member / Chat with admin.
Member-thread entry points use the shared flag glyph; sharing uses share-03.
All platforms retain the core's role and feature permissions for these actions.

### Phone readability and contact requests

Phone settings/action rows have at least 64dp touch targets, 17sp primary text,
14sp supporting text and 24dp icons. Preferences use a 52×32dp switch and
full-width descriptions below controls. Group members have 48dp avatars with
12dp vertical insets. Android typography continues to respect the in-app text
size preference. Linux applies larger row spacing in the mobile layout.

Share this group/channel replaces the Group link action and opens the existing
link management/sharing page; link creation, copy, roles and revocation remain
available there. No duplicate link entry appears in the profile.

Group-member DM invitations arrive as direct chats with an Accept/Reject bar.
They use the contact-id namespace and /_accept member contact; declining silently
deletes the unaccepted contact. Address requests retain /_accept and /_reject.
Request kinds stay distinct even when numeric ids coincide, and member-invitation
events refresh chat lists on every platform. Sending/calling remains blocked
until the request is accepted.

Mac calling explicitly connects microphone input to a sink while remote audio
uses its independent source node. Capture is resampled into 48kHz mono PCM;
missing input retries without voice processing, and conversion failures reach
the UI. The Mac self-test covers 48kHz and 44.1kHz stereo-to-mono capture without
opening a microphone. Live duplex audio still requires a hardware call test.

Mac window controls use a real unified-compact NSToolbar so AppKit applies its inset
traffic-light placement and toolbar window geometry. Message text and captions
reserve an unbreakable invisible tail for the clock, edited label and receipt;
the visible metadata overlays its bottom trailing corner. The text shaper moves
the whole reservation to a new line when it cannot fit. On macOS 13+, bubble
contents measure naturally, then stretch the final text to the widest sibling,
matching Linux's 420pt limit and keeping stamps aligned beneath wider quotes.

Android message text and captions use an empty inline placeholder sized from the
actual metadata text measurements, receipt icon and gaps. It wraps as one unit
with the last line; the visible stamp stays at the bubble's bottom-right. The
placeholder retains the body line height, respects system font scaling and does
not append hidden timestamp characters to message content. Intrinsic bubble
width keeps short messages compact while aligning stamps beneath wider quotes
or attachments. Images put metadata on the preview instead of under the caption. Other media
without captions, invitations and deleted placeholders keep a separate footer. Private notes omit delivery receipts.

Android and Mac image previews place their metadata inside the actual image
bounds, inset 6pt/dp from the right and 4pt/dp from the bottom. A compact black
50%-opacity rounded backing with white text keeps the clock legible over both
light and dark photos. Receipts retain their delivery state. The badge stays
sharp when media is blurred, does not intercept gallery/long-press gestures,
and replaces the separate footer even when the image has a caption.

Image/video bubbles use an even 4pt/dp media bezel, a uniform 16pt/dp outer
corner and 12pt/dp preview corner. Mac and Android sender labels, replies and
captions receive their own text inset instead of widening the frame. Completed videos keep
duration at bottom-left and the stamp at bottom-right; Android continues to
open the existing external player. Mac sidebar headers use 44pt height and
unified-compact window chrome. The dark navigation lens uses a subtle white tint
and optical rim, while text stays above the glass and selected hover stays off.

Connect uses the same three choices on every platform: Connect, New group and
New channel. The form distinguishes a private one-time invitation from joining
with a pasted link; New link is secondary and Connect is primary. Link copy
controls show one ellipsized line while copying the complete value. Mac and
Android keep invitation and form drafts for the page lifetime, guard concurrent
submissions and delete unshared links even if creation finishes after closing.
Calls use a phone glyph for their new-call action.

Public and local display names render literally, including in editor previews.
Bios, member bios, group/channel descriptions and welcome text retain rich text
and Unicode emoji. Cursus markup uses **bold**, *italic* or _italic_, ~strike~
and `code`. The shared parser adapts star markers for the pinned SimpleX core;
stored and transmitted source text stays unchanged. Code and URL tokens retain
their literal asterisks. Mac profile prose uses a bounded async formatting cache.

The Mac media gallery registers arrow handling for the gallery window lifetime,
independent of player/button focus. Navigation reads the current index on every
press; cancelled media loads cannot replace newer content or its loading state.

Linux member details use the same profile hierarchy as Mac: 64pt avatar beside
literal name, public name and role/status, followed by rich bio and grouped
navigation/action rows. Role changes use the existing selection overlay rather
than a stack of one-button-per-role actions. Self profiles remain read-only.
Android member lists use 48dp avatars, 12dp vertical row padding and a minimum
72dp touch target, with 4dp between name and bio.

Sharing a group/channel uses one link page across platforms: explanation,
one-line copy box, sharing action and (group admins only) joining role and
confirmed link deletion. Linux copies the link for sharing; Android and Mac use
their system share sheets. The Invite a contact page contains only invitations.

Android group/channel sharing follows the Mac page: description, single-line
copy box, native Share link action, admin-only joining role and confirmed link
deletion. Linux uses the same hierarchy with a clipboard share action. The
invite-contact form is separate. Missing channel links remain read-only because
the protocol cannot recreate them. Android member lists use 48dp avatars, 12dp
vertical padding and at least 72dp rows. Linux member details use a 64px avatar,
plain identity and role/status, rich bio, and shared navigation/preference rows
for messaging, moderation and member management; own-member pages are read-only.

Phone profile rows use one shared height that accommodates a title and one
preview line at the current text scale. Linux navigation and destructive rows
share the same padding and height; member rows match Android's 48px avatars and
12px vertical inset. Phone member lists show a one-line bio and quiet role label;
the member page holds role/actions. Long editable bios/descriptions are left
aligned, keep the pencil at the top and expose the full text in their editor.
Personal receipt controls sit at the row's outer trailing edge. Only negotiated
chat-permission choices reserve a separate peer-status icon column.

Custom controls and surfaces on Linux and Android use the macOS navigation lens’s 20-point corner radius. Message bubbles and the chat composer retain their dedicated shapes; avatars, switch tracks and circular indicators remain round. Inline message metadata reserves only its measured height, so it cannot increase the body line height or lift the message text.

The shared control radius is 20pt/dp, following the macOS main window’s `.unifiedCompact` toolbar style on Tahoe. The macOS window itself keeps its system-managed geometry. Linux exposes this as `theme::CONTROL_RADIUS`; Android uses `Radius.control`; macOS uses `Radius.control`.


## Cross-platform UI audit — November 2026

Shared components cover the conversation and call lists, chat/group/channel and
member profiles, settings categories and preferences, connection/creation flows,
link sharing and selection overlays. Platform navigation stays native: Android
system Back, macOS keyboard/menu/toolbar behavior, and mouse/touch-aware Linux
controls. The macOS selection lens keeps its native glass treatment.

- Preference cards use a 20pt outer curve, equal 4pt insets and concentric 16pt
  inner hover/ripple clipping. Headings and supporting notes align with row
  content. Notes take the width below controls rather than a narrow side column.
- Floating destination bars use a consistent 64pt/dp height across platforms.
- Navigation/action rows share 20pt glyphs in 24pt slots with a 14pt gap. Phone
  primary/secondary actions and header tools have at least 48dp targets. Desktop
  navigation/action rows have a 44pt minimum while keyboard controls stay compact.
- Section subtitles are visible, including member counts. Profile quick actions
  on Android keep active Star/Mute states visibly selected after the touch ends.
- Shareable links have a one-line preview and an explicit Copy action, a quiet
  border, and platform copy feedback. Invitation management is labeled “Unused
  invitations”; replacing a one-time link is labeled “Replace link”. Existing
  confirmations remain in place. Connect exposes “Use a random profile” as a
  labeled switch with supporting text rather than an unexplained icon.
- Android overlays respect system/keyboard insets and scroll on short windows.
  Form fields show their focus border. Material dialogs use the shared shape
  tokens. macOS icon tools expose accessibility names as well as hover help.
- All platforms share the same dark elevated/hover/border surfaces and incoming
  bubble colors. Ordinary primary, secondary and muted text on standard Linux
  surfaces must meet 4.5:1 contrast in both themes (covered by a palette test).
- Chat composers and message bubbles keep dedicated geometry. Every bubble type,
  including media, has one small sender-side corner, or two when joining the
  next message. Avatars, switches and round indicators retain their purpose.

Review snapshots are generated by `shots::shots_` tests in `target/shots`.
Run these sequentially (`--test-threads=1`) because the headless test harness
shares active theme state. Native macOS appearance still requires a Mac review;
Swift parsing on Linux is not a native build or visual verification.

Destination docks use an equal 5pt/dp inset on every side, 4pt gaps, and a
15pt selection radius concentric with the 20pt outer surface. The inset is
measured from the outer silhouette, including its 1pt border. macOS lens and
tab hit shapes share the same geometry throughout the sliding animation.

## Cross-platform UI audit — second pass (25 November 2026)

- One section component on every platform: heading outside, rows in a panel
  (see “Pages: panels”). Settings categories, profile actions, member lists,
  member pages, invite and share pages all use it.
- Chat permissions say in words what the contact allows (“Alice allows this”)
  under each row, instead of a red / green / gold person glyph. The wording and
  the `Off` / `On` / `Always` labels come from `cursus_core::preferences`
  (`FeatureAllow::label`, `peer_summary`); the bridge sends them as
  `mineLabel` / `contactSummary`. Receipts per contact are “Delivery receipts”
  under **Privacy**.
- Header actions are `accent.soft` rounded squares (radius 12) on Chats,
  Channels and Calls; creation choices use the quick-action tile.
- The member page puts Role on a row with its value; Leave group / channel is red.
- Welcome / lock cards carry the panel border and sit proportionally high on
  phones (room for the keyboard).

### Follow-up (25 November 2026)

- **Choice sheets carry the explanation.** A row that opens a choice is one
  centred line: icon, label, value, chevron. What the choice means, and for
  contact permissions what the contact allows (`FeatureAllow::peer_note`,
  sent as `contactNote`), is a note at the bottom of the sheet (Linux /
  Android) or the last, disabled item of the menu (macOS). Sheet title,
  options and note share one text edge (16 px inside the 4 px panel inset);
  the current option has a check.
- **Member rows** end with the member's role icon as a sign, never a
  control and never the role name. The whole row opens the member page,
  which holds role and moderation actions. Icons: observer eye, member
  person, moderator shield, admin key-person, owner badge.
- **Blocked messages.** Items the core marks `blocked` / `blockedByAdmin`
  show a tombstone ("blocked", "blocked by admin") with Reveal; core owns the
  state and its label. There is no "hide messages for me" action: official
  SimpleX doesn't offer it, so the member page doesn't either.
- **Message info** uses label · value rows in words ("Sent", "Delivered",
  `MessageInfo::status_label`). For your group messages it lists members
  under "Received by" / "Not received yet" / "Not delivered" with avatar and
  name. SimpleX group receipts mean delivered to the member's device; there
  is no separate "read" state in groups.
- **Android composer:** attach, field and mic/send are all 48 dp tall, so
  they share one centre line; the field grows upward.
- **Profile header.** Picture, name, local name and a short description,
  all centred. A paragraph-long description (a line break or over 64
  characters) leaves the hero and gets a "Description" block below the
  actions. Star / Mute / Settings (and Call) are big labelled tiles only on
  phones, spanning the page edge to edge like the panels; wider windows and
  the Mac show them as small icon tools in the page header.
- **Setting help.** The (i) of a setting sits right after its name, as its
  own small button; the summary underneath is plain text and toggles the row
  like the rest of it.
- **Bubbles:** radius 18 (16 around flush media, concentric with the 12
  preview), sender-side joins 6, on every platform. A group sender's name
  sits 4 above the message.
- **Composer (all platforms).** One-line field and its action are 42 tall
  on phones, 36 on desktop, radius 12 like the "+". No bar behind it: a floating field
  (elevated fill, 1 px border, soft shadow, radius 12 like every button)
  with the paperclip inside at its end (✕ while recording), and one
  rounded-square action beside it, the same shape as the list header's
  "+": soft accent mic while empty, solid accent Send once there is text.
- **Chat header.** Tapping the name or picture opens the profile; there is
  no hover tile, no (i) and no support button. Only unread support messages
  add the yellow counter.
- **Message menu reactions** stay on one line, sharing the sheet's width.
- **Member actions** reach members up to your own rank (core
  `roles::can_act_on`; role menu `role_change_choices`, owners can make
  owners).
- **Floating composer.** The composer is layered over the chat history,
  which scrolls underneath it and keeps the composer's measured height
  as room at its end, with no extra history padding: at rest the last
  message sits exactly 8 px (the composer's top inset) above the field, as
  in WhatsApp and Telegram (Linux `measure::height_probe`, Android `onSizeChanged`,
  macOS `safeAreaInset`). Lift per platform: macOS 26 Liquid Glass
  (`FloatingSurface`, opaque card + shadow before 26 or with Reduce
  Transparency); Android Material elevation (6 dp shadow, level 3); Linux an
  iced container shadow. Reply / edit bars, a pasted image, emoji
  suggestions and voice drafts float on their own cards of the same style.
- **Glass.** Floating surfaces are heavily frosted (0.94 dark / 0.95 light
  of the elevated colour; Linux and Android have no backdrop blur, so
  density stands in for it) with a light edge (white 14 % dark / black
  10 % light) and a
  wide soft shadow (y 8, blur 28). The fade is only the bar's 8 px top
  inset: the surface underneath goes from clear to 0.92 across that strip
  and holds, so messages dissolve as they slide under it but a resting last
  message is never dimmed. macOS: Liquid Glass on 26, the system `.regularMaterial` blur
  before; Linux and Android have no backdrop blur, so translucency + fade
  carry the effect.
- **Emoji suggestions.** A fixed 5-row list of `glyph :shortcode:` rows on
  the composer field's own surface, exactly as wide as the field (it ends
  where the field ends, not under the action button), hits glued to the
  field. The card has a 4 px inset; row hover / selection is concentric with
  it (radius 12 − 4 = 8). Selected: accent fill, `accent.on` text. Glyph and
  shortcode are vertically centred in the 36 px row.
- **Reply quotes.** Inside the bubble: an inset card (black 22 %, radius 10,
  8 × 6 padding) with a full-height 3 px rounded accent bar, "You" / "Reply"
  in accent and up to three lines of the original. Tapping it scrolls to the
  original and highlights it for 1.6 s.
