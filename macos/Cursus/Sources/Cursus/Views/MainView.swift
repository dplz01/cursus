import AppKit
import SwiftUI

/// A floating sidebar with native window controls and the destination dock.
struct MainView: View {
    @EnvironmentObject private var model: AppModel
    @State private var sidebarWidth: CGFloat = 310
    @State private var resizeOrigin: CGFloat?

    var body: some View {
        columns
        .background(Theme.app)
        .ignoresSafeArea(.container, edges: .top)
        .background(CallPresentation(calls: model.calls).environmentObject(model))
        .onChange(of: model.showInfo) { showing in
            // The retained conversation must not keep a hidden composer active.
            if showing { NSApp.keyWindow?.makeFirstResponder(nil) }
        }
        .onExitCommand {
            if model.showInfo { model.showInfo = false }
            else if model.support != nil { model.closeSupport() }
            else if model.selectingMessages {
                model.selectingMessages = false
                model.selectedMessages = []
            } else if model.composeMode != .new { model.composeMode = .new }
            else if model.destination == .settings { model.settingsPage = nil }
            else { model.selection = nil }
        }
        .sheet(isPresented: $model.showNewChat) {
            NewChatSheet().environmentObject(model)
        }
        .sheet(isPresented: $model.showPendingLinks) {
            PendingLinksSheet().environmentObject(model)
        }
    }

    private var columns: some View {
        GeometryReader { geometry in
            let width = min(sidebarWidth, max(260, geometry.size.width - 366))
            HStack(spacing: 0) {
                middle
                    .padding(.leading, 8)
                    .padding(.vertical, 8)
                    .frame(width: width)
                // Resize in the gap, without a full-height split-view rule.
                Color.clear
                    .frame(width: 6)
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 1, coordinateSpace: .named("shell"))
                        .onChanged { value in
                            if resizeOrigin == nil { resizeOrigin = width }
                            sidebarWidth = min(400, max(260,
                                min(geometry.size.width - 366,
                                    (resizeOrigin ?? width) + value.translation.width)))
                        }
                        .onEnded { _ in resizeOrigin = nil })
                    .accessibilityLabel("Sidebar width")
                    .accessibilityAdjustableAction { direction in
                        switch direction {
                        case .increment: sidebarWidth = min(400, width + 20)
                        case .decrement: sidebarWidth = max(260, width - 20)
                        @unknown default: break
                        }
                    }
                VStack(spacing: 0) {
                    CallBannerView(calls: model.calls)
                    detail
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .coordinateSpace(name: "shell")
        }
    }

    private var middle: some View {
        // Keep the dock as a stable sibling of the changing page. An overlay
        // attached to the conditional page can recreate it on section changes,
        // losing the previous lens position (Chats/Channels share one branch).
        VStack(spacing: 0) {
            // Native traffic lights remain above the content. This area also
            // provides a drag target without making chat rows draggable.
            WindowDragArea().frame(height: 36).accessibilityHidden(true)
            ZStack(alignment: .bottom) {
                listContent.frame(maxHeight: .infinity)
                NavDock().padding(10)
            }
        }
        .background(Theme.list)
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous)
            .strokeBorder(Theme.border.opacity(0.6), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.08), radius: 10, x: 0, y: 3)
    }

    @ViewBuilder private var listContent: some View {
        switch model.destination {
        case .chats, .channels:
            ChatListView()
        case .calls:
            CallsView(calls: model.calls)
        case .settings:
            SettingsListView()
        }
    }

    @ViewBuilder private var detail: some View {
        switch model.destination {
        case .settings:
            SettingsDetailView()
        case .calls:
            CallDetailView(calls: model.calls)
        case .chats, .channels:
            if let chat = model.selectedChat {
                // The profile is a page of its own (as on Linux), not a strip
                // beside the chat; Back returns to the conversation.
                ZStack {
                    ConversationView(chat: chat).id(model.activeRef ?? chat.ref)
                        .opacity(model.showInfo && !chat.isNotes ? 0 : 1)
                        .allowsHitTesting(!model.showInfo)
                        .accessibilityHidden(model.showInfo)
                    if model.showInfo && !chat.isNotes {
                        ChatInfoView(chat: chat).id(chat.ref)
                    }
                }
            } else {
                Placeholder(icon: model.destination == .channels ? "channels" : "chats",
                            text: "Select a conversation")
            }
        }
    }
}

/// One short sentence on the panel colour (DESIGN_SYSTEM.md → Empty states).
struct Placeholder: View {
    let icon: String
    let text: String

    var body: some View {
        VStack(spacing: 12) {
            IconDisc(icon: icon, size: 56)
            Text(text).font(.cBody).foregroundColor(Theme.secondaryText)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.panel)
    }
}

// MARK: - Floating destination dock

struct NavDock: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    /// Scroll content can pass under the dock, but its final row must be able
    /// to scroll entirely above the 64pt bar and its two 10pt margins.
    static let clearance: CGFloat = 84

    private var refractive: Bool {
        #if compiler(>=6.2) && CURSUS_GLASS
        if #available(macOS 26.0, *) {
            return !reduceTransparency && contrast != .increased
        }
        #endif
        return false
    }

    private var selectedIndex: CGFloat {
        CGFloat(Destination.allCases.firstIndex(of: model.destination) ?? 0)
    }

    private var slideAnimation: Animation? {
        reduceMotion ? nil : .spring(response: 0.42, dampingFraction: 0.82)
    }

    var body: some View {
        ZStack {
            Color.clear.modifier(NavigationChrome())
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            // Keep native glass behind the glyphs so its material never blurs
            // them. A small edge displacement supplies refraction during travel.
            lens
            tabs
        }
        .frame(height: 64)
    }

    /// One persistent surface with an explicitly animated position. Replacing
    /// per-tab glass views can materialize them instead of sliding the lens.
    private var lens: some View {
        GeometryReader { geometry in
            let count = CGFloat(Destination.allCases.count)
            let width = max(0, (geometry.size.width - 10 - 2 * (count - 1)) / count)
            DockSelection(refractive: refractive)
                .frame(width: width, height: 54)
                .position(x: 5 + width / 2 + selectedIndex * (width + 2), y: 32)
                .animation(slideAnimation, value: model.destination)
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private var tabs: some View {
        HStack(spacing: 2) {
            ForEach(Destination.allCases) { dest in
                Button { model.destination = dest } label: {
                    VStack(spacing: 4) {
                        CIcon(dest.icon, size: 21,
                              color: model.destination == dest ? Theme.accent : Theme.secondaryText)
                        Text(dest.title).font(.system(size: 10, weight: model.destination == dest ? .semibold : .regular))
                            .lineLimit(1)
                    }
                    .foregroundColor(model.destination == dest ? Theme.accent : Theme.secondaryText)
                    .modifier(DockEdgeRefraction(
                        position: selectedIndex, target: selectedIndex,
                        tab: CGFloat(Destination.allCases.firstIndex(of: dest) ?? 0),
                        enabled: refractive && !reduceMotion))
                    .animation(slideAnimation, value: model.destination)
                    .frame(maxWidth: .infinity)
                    .frame(height: 54)
                    .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(alignment: .topTrailing) {
                        if badge(for: dest) > 0 {
                            UnreadPill(count: badge(for: dest))
                                .scaleEffect(0.8).offset(x: 0, y: -2)
                        }
                    }
                }
                .buttonStyle(GhostTileStyle(radius: 12, hoverEnabled: model.destination != dest))
                .help(dest.title)
                .accessibilityLabel(dest.title)
                .accessibilityValue(model.destination == dest ? "Selected" : "")
            }
        }
        .padding(5)
    }

    private func badge(for dest: Destination) -> Int {
        switch dest {
        case .chats:
            return model.chats.filter { !$0.isChannel && !$0.muted && $0.hasUnread }.count
        case .channels:
            return model.chats.filter { $0.isChannel && !$0.muted && $0.hasUnread }.count
        default:
            return 0
        }
    }
}

/// Animate the optical displacement independently of the glass material.
/// Only glyphs crossed by a moving lens edge stretch slightly; at rest this
/// is exactly the identity transform, with no blur, duplicate text or scaling.
private struct DockEdgeRefraction: AnimatableModifier {
    var position: CGFloat
    let target: CGFloat
    let tab: CGFloat
    let enabled: Bool

    var animatableData: CGFloat {
        get { position }
        set { position = newValue }
    }

    func body(content: Content) -> some View {
        let distance = min(abs(tab - position - 0.5), abs(tab - position + 0.5))
        let proximity = max(0, 1 - distance / 0.28)
        let edge = proximity * proximity * (3 - 2 * proximity)
        let moving = min(1, abs(target - position) * 10)
        let strength = enabled ? edge * moving : 0
        let direction: CGFloat = tab < position ? -1 : 1
        content
            .scaleEffect(x: 1 + 0.055 * strength, y: 1 + 0.025 * strength)
            .offset(x: direction * 1.2 * strength)
    }
}

private struct DockSelection: View {
    @Environment(\.colorScheme) private var colorScheme
    let refractive: Bool

    @ViewBuilder var body: some View {
        #if compiler(>=6.2) && CURSUS_GLASS
        if #available(macOS 26.0, *), refractive {
            if colorScheme == .light {
                // Clear glass on white needs a tint and an optical rim to
                // remain legible. Keep the existing dark material untouched.
                Color.clear
                    .glassEffect(.clear.tint(Theme.accent.opacity(0.10)),
                                 in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(LinearGradient(
                            colors: [.white.opacity(0.95), Theme.accent.opacity(0.28), .white.opacity(0.75)],
                            startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 1))
                    .shadow(color: Theme.accent.opacity(0.14), radius: 3, x: 0, y: 1)
            } else {
                Color.clear.glassEffect(.clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
        } else { fallback }
        #else
        fallback
        #endif
    }

    private var fallback: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.accent.opacity(0.16))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Theme.accent.opacity(0.35), lineWidth: 1))
    }
}

// MARK: - Chat list

struct ChatListView: View {
    @EnvironmentObject private var model: AppModel
    @State private var search = ""

    private var rows: [Chat] {
        let base = model.visibleChats
        let q = search.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return base }
        return base.filter { $0.title.localizedCaseInsensitiveContains(q) || $0.preview.localizedCaseInsensitiveContains(q) }
    }

    var body: some View {
        VStack(spacing: 0) {
            PaneHeader(title: model.destination.title, background: Theme.list) {
                if !model.pendingInvitations.isEmpty {
                    PendingLinksButton(count: model.pendingInvitations.count) { model.showPendingLinks = true }
                }
                CreationButton(help: "New chat (⌘N)") { model.showNewChat = true }
            }
            SearchField(text: $search)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            list
        }
        .background(Theme.list)
    }

    private var list: some View {
        ScrollView {
            LazyVStack(spacing: 2) {
                ForEach(rows) { chat in
                    ChatRow(chat: chat, selected: model.selection == chat.ref) {
                        model.selection = chat.ref
                    }
                    .contextMenu { ChatMenu(chat: chat) }
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .padding(.bottom, NavDock.clearance)
        }
        .background(Theme.list)
        .overlay {
            if model.visibleChats.isEmpty {
                VStack(spacing: 12) {
                    Text(model.destination == .channels ? "No channels yet." : "No conversations yet.")
                        .font(.cBody)
                        .foregroundColor(Theme.secondaryText)
                    if model.destination == .chats {
                        Button("Add contact") { model.showNewChat = true }
                            .buttonStyle(PrimaryButtonStyle(small: true))
                    }
                }
            }
        }
    }
}

/// Rounded search field of the chat list (Linux: the list's search box).
struct SearchField: View {
    @Binding var text: String
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundColor(Theme.mutedText)
            TextField("Search", text: $text)
                .textFieldStyle(.plain)
                .font(.cBody)
                .focused($focused)
            if !text.isEmpty {
                Button { text = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundColor(Theme.mutedText)
                }
                .buttonStyle(.plain)
                .help("Clear")
            }
        }
        .padding(.vertical, 7)
        .padding(.horizontal, 10)
        .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(Theme.fieldFill))
        .overlay(RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
            .strokeBorder(focused ? Theme.accent : Theme.border, lineWidth: 1))
    }
}

struct ChatMenu: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    var body: some View {
        if chat.hasUnread {
            Button("Mark as read") { Task { await model.markRead(chat.ref) } }
        } else {
            Button("Mark as unread") { Task { await model.markUnread(chat) } }
        }
        Button(chat.favorite ? "Remove from favorites" : "Add to favorites") {
            Task { await model.setFavorite(chat, !chat.favorite) }
        }
        Button(chat.muted ? "Unmute" : "Mute") {
            Task { await model.setMuted(chat, !chat.muted) }
        }
        if !chat.isNotes && chat.pendingRequestId == nil {
            Button(chat.isGroup ? (chat.isChannel ? "Channel info" : "Group info") : "Contact info") {
                model.selection = chat.ref
                if !model.showInfo { model.toggleInfo() }
            }
        }
        Divider()
        Button("Clear messages…") {
            if confirm("Clear all messages in “\(chat.title)”?", "Messages are deleted from this Mac only.") {
                Task { await model.clear(chat) }
            }
        }
        Button("Delete chat…") {
            if confirm("Delete “\(chat.title)”?", "The conversation and its messages are deleted.") {
                Task { await model.delete(chat) }
            }
        }
    }

    private func confirm(_ title: String, _ detail: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Delete")
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }
}

/// 44px avatar · name / two-line preview · stamp, star, mute, unread pill.
struct ChatRow: View {
    let chat: Chat
    let selected: Bool
    let action: () -> Void
    @AppStorage(Prefs.chatPreviews) private var previews = true

    private var previewText: String {
        if chat.pendingRequestId != nil { return "Wants to connect" }
        return previews ? chat.preview : ""
    }

    var body: some View {
        Button(action: action) {
            HStack(alignment: .top, spacing: 10) {
                Avatar(image: chat.image, name: chat.title, size: 44, kind: chat.avatarKind)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        Text(chat.title)
                            .font(chat.hasUnread ? .cBodyBold : .cBody)
                            .foregroundColor(Theme.primaryText)
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        if let date = chat.lastDate {
                            Text(Dates.listStamp(date))
                                .font(.cMeta)
                                .foregroundColor(chat.hasUnread && !chat.muted ? Theme.accent : Theme.mutedText)
                        }
                    }
                    HStack(alignment: .top, spacing: 4) {
                        Text(previewText)
                            .font(.cMeta)
                            .foregroundColor(chat.pendingRequestId != nil ? Theme.gold : Theme.secondaryText)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .topLeading)
                        if chat.favorite {
                            CIcon("star-filled", size: 14, color: Theme.accent).padding(.top, 1)
                        }
                        if chat.muted {
                            CIcon("notifications-off", size: 14, color: Theme.mutedText).padding(.top, 1)
                        }
                        if chat.hasUnread {
                            UnreadPill(count: chat.unread, muted: chat.muted)
                        }
                    }
                }
            }
            .padding(.vertical, 8)
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(GhostTileStyle(selected: selected))
    }
}
