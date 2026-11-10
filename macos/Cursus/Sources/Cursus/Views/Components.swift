import AppKit
import SwiftUI

// The Cursus component set (docs/DESIGN_SYSTEM.md → Components), built once
// here. Screens compose these; they don't style buttons themselves. Native
// macOS pieces (window, toolbar, menus, sheets, alerts, context menus, text
// editing) stay native.

/// Tracks the pointer for hover fills (ButtonStyle can't hold state itself).
private struct Hovering<Content: View>: View {
    let content: (Bool) -> Content
    @State private var hover = false

    var body: some View {
        content(hover).onHover { hover = $0 }
    }
}

// MARK: - Buttons

/// The one main action of a screen: accent fill, radius 10.
struct PrimaryButtonStyle: ButtonStyle {
    var small = false
    /// Stretch to the available width.
    var stretch = false

    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration, small: small, stretch: stretch)
    }

    private struct Inner: View {
        let configuration: ButtonStyleConfiguration
        let small: Bool
        let stretch: Bool
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            Hovering { hover in
                configuration.label
                    .font(small ? .cMeta : .cBody)
                    .foregroundColor(enabled ? Theme.accentOn : Theme.mutedText)
                    .padding(.vertical, small ? 6 : 9)
                    .padding(.horizontal, small ? 12 : 16)
                    .frame(maxWidth: stretch ? .infinity : nil)
                    .background(
                        RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                            .fill(!enabled ? Theme.disabledFill
                                  : configuration.isPressed ? Theme.accentPress
                                  : hover ? Theme.accentHover : Theme.accent))
                    .contentShape(Rectangle())
            }
        }
    }
}

/// Neutral alternatives (Cancel, choice chips): field fill + hairline.
struct SecondaryButtonStyle: ButtonStyle {
    var small = false

    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration, small: small)
    }

    private struct Inner: View {
        let configuration: ButtonStyleConfiguration
        let small: Bool
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            Hovering { hover in
                configuration.label
                    .font(small ? .cMeta : .cBody)
                    .foregroundColor(enabled ? Theme.primaryText : Theme.mutedText)
                    .padding(.vertical, small ? 6 : 9)
                    .padding(.horizontal, small ? 12 : 16)
                    .background(
                        RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                            .fill(configuration.isPressed ? Theme.accentSoft
                                  : hover && enabled ? Theme.hover : Theme.fieldFill))
                    .overlay(
                        RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                            .strokeBorder(Theme.border, lineWidth: 1))
                    .contentShape(Rectangle())
            }
        }
    }
}

/// Final confirmation of something destructive.
struct DangerButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Hovering { hover in
            configuration.label
                .font(.cBody)
                .foregroundColor(.white)
                .padding(.vertical, 9)
                .padding(.horizontal, 16)
                .background(
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                        .fill(configuration.isPressed ? Theme.dangerPress
                              : hover ? Theme.dangerHover : Theme.danger))
                .contentShape(Rectangle())
        }
    }
}

/// Ghost tile: no fill or border at rest; the shape shows on hover.
/// Used by action tiles, navigation rows and list rows.
struct GhostTileStyle: ButtonStyle {
    var danger = false
    var selected = false
    var radius: CGFloat = Radius.lg

    func makeBody(configuration: Configuration) -> some View {
        Inner(configuration: configuration, style: self)
    }

    private struct Inner: View {
        let configuration: ButtonStyleConfiguration
        let style: GhostTileStyle
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            Hovering { hover in
                configuration.label
                    .contentShape(Rectangle())
                    .background(
                        RoundedRectangle(cornerRadius: style.radius, style: .continuous)
                            .fill(style.tileFill(hover: hover && enabled, pressed: configuration.isPressed)))
            }
        }
    }

    fileprivate func tileFill(hover: Bool, pressed: Bool) -> Color {
        if danger {
            return pressed ? Theme.dangerSoftPress : hover ? Theme.dangerSoft : Color.clear
        }
        if pressed { return Theme.accentSoftPress }
        if selected { return hover ? Theme.accentSoftHover : Theme.accentSoft }
        return hover ? Theme.hover : Color.clear
    }
}

/// Icon-only tool: 32×32 hit, glyph secondary → primary on hover.
struct IconToolStyle: ButtonStyle {
    var active = false

    func makeBody(configuration: Configuration) -> some View {
        Hovering { hover in
            configuration.label
                .foregroundColor(active ? Theme.accent : hover ? Theme.primaryText : Theme.secondaryText)
                .frame(width: 32, height: 32)
                .background(
                    Circle().fill(configuration.isPressed ? Theme.accentSoft : hover ? Theme.hover : Color.clear))
                .contentShape(Circle())
        }
    }
}

struct IconTool: View {
    let icon: String
    let help: String
    var active = false
    var size: CGFloat = 20
    let action: () -> Void

    var body: some View {
        Button(action: action) { CIcon(icon, size: size) }
            .buttonStyle(IconToolStyle(active: active))
            .help(help)
    }
}

/// Small pencil after an editable text (same size as the Linux one).
struct PencilButton: View {
    var help = "Edit"
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            CIcon("edit-3", size: 14)
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundColor(Theme.mutedText)
        .help(help)
    }
}

// MARK: - Bubble shape

/// Message bubble: radius 16 except on the sender's side (right for sent,
/// left for received), where the top corner is always small and the bottom
/// one too on every bubble of a run but its last — the rule of every Cursus
/// app. macOS 12 has no per-corner rounded rectangle.
struct BubbleShape: Shape {
    var outgoing: Bool
    var runContinues: Bool
    var radius: CGFloat = Radius.bubble
    var small: CGFloat = 6

    func path(in rect: CGRect) -> Path {
        let r = min(radius, rect.width / 2, rect.height / 2)
        let s = min(small, r)
        let bottom = runContinues ? s : r
        let tl = outgoing ? r : s
        let tr = outgoing ? s : r
        let br = outgoing ? bottom : r
        let bl = outgoing ? r : bottom
        var p = Path()
        p.move(to: CGPoint(x: rect.minX + tl, y: rect.minY))
        p.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
                 tangent2End: CGPoint(x: rect.maxX, y: rect.maxY), radius: tr)
        p.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.maxY),
                 tangent2End: CGPoint(x: rect.minX, y: rect.maxY), radius: br)
        p.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.maxY),
                 tangent2End: CGPoint(x: rect.minX, y: rect.minY), radius: bl)
        p.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.minY),
                 tangent2End: CGPoint(x: rect.maxX, y: rect.minY), radius: tl)
        p.closeSubpath()
        return p
    }
}

// MARK: - Choice menu

/// One option of a `ChoiceMenu`.
struct Choice<Value: Hashable>: Identifiable {
    let value: Value
    let label: String
    var id: Value { value }

    init(_ value: Value, _ label: String) {
        self.value = value
        self.label = label
    }
}

/// Linux "choice chip": shows the current value; click for the options
/// (a native menu, the current one ticked). Replaces menu-style Pickers,
/// which came out greyed out / blank on macOS 12.
struct ChoiceMenu<Value: Hashable>: View {
    let options: [Choice<Value>]
    let selection: Value
    var enabled = true
    let select: (Value) -> Void

    private var current: String {
        options.first(where: { $0.value == selection })?.label ?? ""
    }

    var body: some View {
        Menu {
            ForEach(options) { option in
                Button {
                    if option.value != selection { select(option.value) }
                } label: {
                    if option.value == selection {
                        Label(option.label, systemImage: "checkmark")
                    } else {
                        Text(option.label)
                    }
                }
            }
        } label: {
            Text(current).font(.cMeta)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .disabled(!enabled)
        .foregroundColor(Theme.accent)
    }
}

// MARK: - Pane header

/// Cursus header at the top of a pane (Linux: list title, chat header,
/// profile / settings page title): Back (also Esc), optional leading
/// content (an avatar), title + subtitle, actions on the right; a hairline
/// below. The window's own title bar only carries the traffic lights and
/// "Cursus", so every pane brings its header, as on Linux.
struct PaneHeader<Leading: View, Actions: View>: View {
    let title: String
    var subtitle: String?
    var back: (() -> Void)?
    var background: Color
    /// Click on the avatar + title (chat header → profile page).
    var open: (() -> Void)?
    var openHelp: String?
    let leading: Leading
    let actions: Actions

    init(title: String, subtitle: String? = nil, back: (() -> Void)? = nil, background: Color = Theme.panel,
         open: (() -> Void)? = nil, openHelp: String? = nil,
         @ViewBuilder leading: () -> Leading, @ViewBuilder actions: () -> Actions) {
        self.title = title
        self.subtitle = subtitle
        self.back = back
        self.background = background
        self.open = open
        self.openHelp = openHelp
        self.leading = leading()
        self.actions = actions()
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                if let back = back {
                    Button(action: back) { CIcon("back", size: 18) }
                        .buttonStyle(IconToolStyle())
                        .keyboardShortcut(.cancelAction)
                        .help("Back (Esc)")
                }
                if let open = open {
                    Button(action: open) {
                        identity.contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(openHelp ?? "")
                } else {
                    identity
                }
                Spacer(minLength: 8)
                HStack(spacing: 4) { actions }
            }
            .padding(.horizontal, 14)
            .frame(height: 56)
            Hairline()
        }
        .background(background)
    }

    private var identity: some View {
        HStack(spacing: 10) {
            leading
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.cTitle)
                    .foregroundColor(Theme.primaryText)
                    .lineLimit(1)
                if let subtitle = subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.cMeta)
                        .foregroundColor(Theme.secondaryText)
                        .lineLimit(1)
                }
            }
        }
    }
}

extension PaneHeader where Leading == EmptyView {
    init(title: String, subtitle: String? = nil, back: (() -> Void)? = nil, background: Color = Theme.panel,
         @ViewBuilder actions: () -> Actions) {
        self.init(title: title, subtitle: subtitle, back: back, background: background,
                  leading: { EmptyView() }, actions: actions)
    }
}

extension PaneHeader where Leading == EmptyView, Actions == EmptyView {
    init(title: String, subtitle: String? = nil, back: (() -> Void)? = nil, background: Color = Theme.panel) {
        self.init(title: title, subtitle: subtitle, back: back, background: background,
                  leading: { EmptyView() }, actions: { EmptyView() })
    }
}

// MARK: - Discs, tiles, rows

/// Round tinted disc around a glyph (card titles, tiles, nav rows).
struct IconDisc: View {
    let icon: String
    var size: CGFloat = 32
    var tint: Color = Theme.accent
    var fill: Color = Theme.accentSoft

    var body: some View {
        CIcon(icon, size: size * 0.52, color: tint)
            .frame(width: size, height: size)
            .background(Circle().fill(fill))
    }
}

/// Clear / leave / delete: 32px disc + body label. `stacked`: disc above
/// the label, filling its share of a row of tiles (wide pages, as on Linux).
struct ActionTile: View {
    let icon: String
    let title: String
    var danger = false
    var stacked = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if stacked {
                    VStack(spacing: 8) {
                        disc
                        label
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 16)
                    .padding(.horizontal, 8)
                } else {
                    HStack(spacing: 10) {
                        disc
                        label
                        Spacer(minLength: 0)
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 10)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(GhostTileStyle(danger: danger))
    }

    private var disc: some View {
        IconDisc(icon: icon, tint: danger ? Theme.danger : Theme.accent,
                 fill: danger ? Theme.dangerSoft : Theme.accentSoft)
    }

    private var label: some View {
        Text(title)
            .font(.cBody)
            .foregroundColor(danger ? Theme.danger : Theme.primaryText)
            .lineLimit(1)
    }
}

/// Opens another page: disc · title / subtitle · chevron.
struct NavRow: View {
    let icon: String
    let title: String
    var subtitle: String? = nil
    var selected = false
    var trailing: String? = "chevron"
    /// In a `Card(padding: 0)`: the row spans the card edge to edge.
    var flush = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                IconDisc(icon: icon)
                VStack(alignment: .leading, spacing: 1) {
                    Text(title).font(.cBody).foregroundColor(Theme.primaryText)
                    if let subtitle = subtitle {
                        Text(subtitle).font(.cMeta).foregroundColor(Theme.secondaryText).lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
                if trailing == "chevron" {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundColor(Theme.mutedText)
                } else if let trailing = trailing {
                    CIcon(trailing, size: 16, color: Theme.mutedText)
                }
            }
            .padding(.vertical, flush ? 12 : 8)
            .padding(.horizontal, flush ? 14 : 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(GhostTileStyle(selected: selected))
    }
}

// MARK: - Cards and pages

/// `bg.elevated`, 1px border, radius 12. `padding: 0` for a card made only
/// of buttons (flush rows / tiles): the whole card surface is clickable.
struct Card<Content: View>: View {
    var padding: CGFloat = 12
    var spacing: CGFloat = 10
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: spacing) {
            content
        }
        .padding(padding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.elevated))
        .overlay(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).strokeBorder(Theme.border, lineWidth: 1))
    }
}

/// 36px disc, title and a one-line explanation.
struct CardTitle: View {
    let icon: String
    let title: String
    var subtitle: String? = nil

    var body: some View {
        HStack(spacing: 10) {
            IconDisc(icon: icon, size: 36)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.cBodyBold).foregroundColor(Theme.primaryText)
                if let subtitle = subtitle {
                    Text(subtitle)
                        .font(.cMeta)
                        .foregroundColor(Theme.secondaryText)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
        }
    }
}

/// Meta-size muted label above a group of rows.
struct SectionLabel: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.cMeta).foregroundColor(Theme.mutedText)
            .padding(.horizontal, 8)
            .padding(.top, 4)
    }
}

/// `[icon] label ……… control` (settings and preference rows).
struct PrefRow<Control: View>: View {
    var icon: String? = nil
    let label: String
    var note: String? = nil
    @ViewBuilder let control: Control

    var body: some View {
        HStack(spacing: 10) {
            if let icon = icon {
                CIcon(icon, size: 20, color: Theme.secondaryText).frame(width: 24)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(label).font(.cBody).foregroundColor(Theme.primaryText)
                if let note = note {
                    Text(note).font(.cMeta).foregroundColor(Theme.secondaryText)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 8)
            control
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
    }
}

/// A scrolling column of cards, centred and capped at 760pt.
struct Page<Content: View>: View {
    var background: Color = Theme.panel
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                content
            }
            .padding(16)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
        }
        .background(background)
    }
}

// MARK: - Fields

/// Standard field: radius 10, field fill, border → accent when focused.
struct CursusField: View {
    let placeholder: String
    @Binding var text: String
    var secure = false
    var onSubmit: () -> Void = {}
    @FocusState private var focused: Bool

    var body: some View {
        Group {
            if secure {
                SecureField(placeholder, text: $text)
            } else {
                TextField(placeholder, text: $text)
            }
        }
        .textFieldStyle(.plain)
        .font(.cBody)
        .focused($focused)
        .onSubmit(onSubmit)
        .padding(.vertical, 8)
        .padding(.horizontal, 12)
        .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(Theme.fieldFill))
        .overlay(
            RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                .strokeBorder(focused ? Theme.accent : Theme.border, lineWidth: 1))
    }
}

/// Rounded inline field (local name under a profile name): radius 12, 6×12.
struct RoundedField: View {
    let placeholder: String
    @Binding var text: String
    var onCommit: () -> Void = {}
    @FocusState private var focused: Bool

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.cBody)
            .focused($focused)
            .onSubmit(onCommit)
            .onChange(of: focused) { now in
                if !now { onCommit() }
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 12)
            .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.fieldFill))
            .overlay(
                RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                    .strokeBorder(focused ? Theme.accent : Color.clear, lineWidth: 1))
    }
}

/// A shareable link: the whole box copies.
struct LinkBox: View {
    let text: String
    var copyText: String? = nil
    @State private var copied = false

    var body: some View {
        Button {
            Clipboard.copy(copyText ?? text)
            copied = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) { copied = false }
        } label: {
            HStack(alignment: .center, spacing: 10) {
                Text(copied ? "Copied to clipboard" : text)
                    .font(.cMeta.monospaced())
                    .foregroundColor(Theme.accent)
                    .lineLimit(3)
                    .truncationMode(.middle)
                    .multilineTextAlignment(.leading)
                    .frame(maxWidth: .infinity, alignment: .leading)
                CIcon(copied ? "check" : "copy", size: 18, color: Theme.accent)
            }
            .padding(.vertical, 10)
            .padding(.horizontal, 12)
            .background(Hovering { hover in
                RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                    .fill(hover ? Theme.accentSoftHover : Theme.accentSoft)
            })
            .overlay(
                RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                    .strokeBorder(Theme.accent, lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Copy link")
    }
}

/// Unread count: accent on accent.soft (muted: grey).
struct UnreadPill: View {
    let count: Int
    var muted = false

    var body: some View {
        Text(count > 0 ? (count > 999 ? "999+" : "\(count)") : " ")
            .font(.system(size: 11, weight: .semibold))
            .foregroundColor(muted ? Theme.secondaryText : Theme.accent)
            .padding(.horizontal, 6)
            .frame(minWidth: 20, minHeight: 18)
            .background(Capsule().fill(muted ? Theme.hover : Theme.accentSoft))
    }
}

/// 1px hairline in the border colour (between panes, under headers).
struct Hairline: View {
    var vertical = false

    var body: some View {
        Rectangle()
            .fill(Theme.border)
            .frame(width: vertical ? 1 : nil, height: vertical ? nil : 1)
    }
}

// MARK: - Avatar

enum AvatarKind {
    case contact, group, channel, notes, initial
}

/// Profile picture, or Cursus's default disc: `bg.elevated` with a hairline
/// and a person / group / antenna glyph (or an initial).
struct Avatar: View {
    let image: String?
    let name: String
    var size: CGFloat = 36
    var kind: AvatarKind = .initial

    var body: some View {
        Group {
            if let nsImage = Images.fromDataURL(image) {
                Image(nsImage: nsImage).resizable().scaledToFill()
                    .frame(width: size, height: size)
                    .clipShape(Circle())
            } else {
                fallback
                    .frame(width: size, height: size)
                    .background(Circle().fill(kind == .notes ? Theme.accentSoft : Theme.elevated))
                    .overlay(Circle().strokeBorder(kind == .notes ? Color.clear : Theme.border, lineWidth: 1))
            }
        }
        .frame(width: size, height: size)
    }

    @ViewBuilder private var fallback: some View {
        let glyph = min(max(size * 0.52, 14), 36)
        switch kind {
        case .contact: CIcon("user-alt-1", size: glyph, color: Theme.secondaryText)
        case .group: CIcon("user-group", size: glyph, color: Theme.secondaryText)
        case .channel: CIcon("channels", size: glyph, color: Theme.secondaryText)
        case .notes: CIcon("edit-3", size: glyph, color: Theme.accent)
        case .initial:
            Text(initial)
                .font(.system(size: size * 0.42))
                .foregroundColor(Theme.primaryText)
        }
    }

    private var initial: String {
        name.trimmingCharacters(in: .whitespaces).first.map { String($0).uppercased() } ?? "?"
    }
}

extension Chat {
    var avatarKind: AvatarKind {
        if isNotes { return .notes }
        if isChannel { return .channel }
        if isGroup { return .group }
        return .contact
    }
}

// MARK: - Confirm inside a card

/// A destructive question asked in place: the question, Cancel and a solid
/// danger button (DESIGN_SYSTEM.md → Overlays and menus).
struct InlineConfirm: View {
    let question: String
    var detail: String? = nil
    let confirmTitle: String
    let onCancel: () -> Void
    let onConfirm: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(question).font(.cBodyBold).foregroundColor(Theme.primaryText)
            if let detail = detail {
                Text(detail).font(.cMeta).foregroundColor(Theme.secondaryText)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 8) {
                Spacer()
                Button("Cancel", action: onCancel)
                    .buttonStyle(SecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(confirmTitle, action: onConfirm)
                    .buttonStyle(DangerButtonStyle())
            }
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 2)
    }
}
