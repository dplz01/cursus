import AppKit
import SwiftUI

/// A native split view with Cursus's floating destination dock below the list.
struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            CallBannerView(calls: model.calls)
            columns
        }
        .background(Theme.app)
        .background(CallPresentation(calls: model.calls).environmentObject(model))
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

    @ViewBuilder private var columns: some View {
        #if compiler(>=5.7) && CURSUS_MODERN_SHELL
        if #available(macOS 13.0, *) {
            NavigationSplitView {
                middle
                    .navigationSplitViewColumnWidth(min: 260, ideal: 310, max: 400)
            } detail: {
                detail.frame(minWidth: 360)
            }
            .navigationSplitViewStyle(.balanced)
        } else { legacyColumns }
        #else
        legacyColumns
        #endif
    }

    private var legacyColumns: some View {
        // Older Intel Macs retain a native resizable split without resolving
        // symbols missing from their SDK, or an architecture-specific UI fork.
        HSplitView {
            middle.frame(minWidth: 260, idealWidth: 310, maxWidth: 400)
            detail.frame(minWidth: 360)
        }
    }

    private var middle: some View {
        listContent
            .frame(maxHeight: .infinity)
            .background(Theme.list)
            .overlay(alignment: .bottom) {
                NavDock().padding(10)
            }
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
                if model.showInfo && !chat.isNotes {
                    ChatInfoView(chat: chat).id(chat.ref)
                } else {
                    // A support thread is its own conversation (fresh scroll state).
                    ConversationView(chat: chat).id(model.activeRef ?? chat.ref)
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

    var body: some View {
        ZStack {
            Color.clear.modifier(NavigationChrome())
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            if !refractive { lens }
            tabs
            // Clear glass belongs above the glyphs: only then can its moving
            // edge refract them. The opaque accessibility fallback stays below.
            if refractive { lens }
        }
        .frame(height: 64)
    }

    /// One persistent surface with an explicitly animated position. Replacing
    /// per-tab glass views can materialize them instead of sliding the lens.
    private var lens: some View {
        GeometryReader { geometry in
            let count = CGFloat(Destination.allCases.count)
            let width = max(0, (geometry.size.width - 10 - 2 * (count - 1)) / count)
            let index = CGFloat(Destination.allCases.firstIndex(of: model.destination) ?? 0)
            DockSelection(refractive: refractive)
                .frame(width: width, height: 54)
                .position(x: 5 + width / 2 + index * (width + 2), y: 32)
                .animation(reduceMotion ? nil : .spring(response: 0.42, dampingFraction: 0.82),
                           value: model.destination)
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
                    .frame(maxWidth: .infinity)
                    .frame(height: 54)
                    .contentShape(Capsule())
                    .overlay(alignment: .topTrailing) {
                        if badge(for: dest) > 0 {
                            UnreadPill(count: badge(for: dest))
                                .scaleEffect(0.8).offset(x: 0, y: -2)
                        }
                    }
                }
                .buttonStyle(GhostTileStyle(radius: 27))
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

private struct DockSelection: View {
    let refractive: Bool

    @ViewBuilder var body: some View {
        #if compiler(>=6.2) && CURSUS_GLASS
        if #available(macOS 26.0, *), refractive {
            Color.clear.glassEffect(.clear, in: Capsule())
        } else { fallback }
        #else
        fallback
        #endif
    }

    private var fallback: some View {
        Capsule().fill(Theme.accent.opacity(0.16))
            .overlay(Capsule().strokeBorder(Theme.accent.opacity(0.35), lineWidth: 1))
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
                IconTool(icon: "plus", help: "New chat (⌘N)", size: 18) { model.showNewChat = true }
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
