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
| Android (Compose) | Bottom destination dock below list | `NavigationBar` |
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
| Address | create / show / hide, share on profile, auto-accept, delete |
| Privacy & Security | app lock and unlocking at start · on screen (previews, blur, screenshots on Android) · delivery receipts · links · downloads and IP protection · stored-file encryption |
| Notifications | alerts, hide message text · background (tray on Linux and macOS, background service on Android) |
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
- Multi-state preference buttons and their choice menus retain the earlier boxed-value styling.

- SimpleX naming validation still applies to public names: the first character must be a letter, number or supported emoji, and some punctuation is prohibited. The editor explains this and offers inline bold/italic formatting after the first word; descriptions and local aliases are not restricted by those public-name rules.

- Group/channel short descriptions use the same multiline editor, sizing, scrolling and rich-text preview as welcome messages. Existing short-description length limits remain in force.

## macOS chrome

Mac navigation uses the shared Cursus colors and glyphs inside a floating
rounded dock over the bottom of the list in a native resizable list/detail split.
The opaque bar has 18pt corners; selection, hover and hit shapes use 12pt
corners, matching the Linux and Android navigation bars.
The list viewport extends behind the dock. Chats, calls and settings add 84pt
of scroll-content padding so the last row can scroll completely above it.
The dock is a stable sibling of the changing page so all destination pairs
retain their animation state. One persistent selection lens moves between the
four destinations with an explicit spring-animated position; it is not replaced
or crossfaded per tab.
Pointer hover retains the ordinary Cursus hover fill and does not move the lens.
The dock itself is opaque, using the shared Cursus surface and border as on
Linux. Only the selection lens uses native clear Liquid Glass on macOS 26.
The glass stays below the controls so its material cannot blur icons or text.
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

Mac window controls use a real unified NSToolbar so AppKit applies its inset
traffic-light placement and toolbar window geometry. Message text and captions
reserve an unbreakable invisible tail for the clock, edited label and receipt;
the visible metadata overlays its bottom trailing corner. The text shaper moves
the whole reservation to a new line when it cannot fit. On macOS 13+, bubble
contents measure naturally, then stretch the final text to the widest sibling,
matching Linux's 420pt limit and keeping stamps aligned beneath wider quotes.
