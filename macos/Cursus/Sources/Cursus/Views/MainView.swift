import AppKit
import SwiftUI

/// Cursus's nav rail | the macOS split view (list | detail). The rail is
/// Cursus's own (four destinations, Settings at the bottom); the split
/// view, toolbar, search field and menus stay native.
struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            NavRail()
            Hairline(vertical: true)
            NavigationView {
                middle
                    .frame(minWidth: 260, idealWidth: 300)
                detail
                    .frame(minWidth: 380)
            }
        }
        .background(Theme.app)
        .sheet(isPresented: $model.showNewChat) {
            NewChatSheet().environmentObject(model)
        }
    }

    @ViewBuilder private var middle: some View {
        switch model.destination {
        case .chats, .channels:
            ChatListView()
        case .calls:
            CallsView()
        case .settings:
            SettingsListView()
        }
    }

    @ViewBuilder private var detail: some View {
        switch model.destination {
        case .settings:
            SettingsDetailView()
        case .calls:
            Placeholder(icon: "calls", text: "Voice calls are coming to Cursus for Mac.")
        case .chats, .channels:
            if let chat = model.selectedChat {
                // The profile is a page of its own (as on Linux), not a strip
                // beside the chat; Back returns to the conversation.
                if model.showInfo && !chat.isNotes {
                    ChatInfoView(chat: chat).id(chat.ref)
                } else {
                    ConversationView(chat: chat).id(chat.ref)
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

// MARK: - Nav rail

struct NavRail: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 8) {
            ForEach([Destination.chats, .channels, .calls]) { dest in
                NavDestinationButton(dest: dest, badge: badge(for: dest))
            }
            Spacer(minLength: 8)
            NavDestinationButton(dest: .settings, badge: 0)
            if let user = model.user {
                Button { model.destination = .settings; model.settingsPage = .profile } label: {
                    Avatar(image: user.image, name: user.displayName, size: 32)
                }
                .buttonStyle(.plain)
                .help(user.displayName)
                .padding(.top, 2)
            }
        }
        .padding(.vertical, 12)
        .padding(.horizontal, 8)
        .frame(width: 76)
        .frame(maxHeight: .infinity)
        .background(Theme.sidebar)
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

private struct NavDestinationButton: View {
    @EnvironmentObject private var model: AppModel
    let dest: Destination
    let badge: Int

    private var active: Bool { model.destination == dest }

    var body: some View {
        Button { model.destination = dest } label: {
            VStack(spacing: 3) {
                CIcon(dest.icon, size: 22)
                Text(dest.title).font(.system(size: 10, weight: active ? .semibold : .regular))
            }
            .foregroundColor(active ? Theme.accent : Theme.secondaryText)
            .frame(width: 60, height: 52)
            .overlay(alignment: .topTrailing) {
                if badge > 0 {
                    UnreadPill(count: badge)
                        .scaleEffect(0.85)
                        .offset(x: 2, y: -2)
                }
            }
        }
        .buttonStyle(GhostTileStyle(selected: active))
        .help(dest.title)
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
                        Text(chat.pendingRequestId != nil ? "Wants to connect" : chat.preview)
                            .font(.cMeta)
                            .foregroundColor(chat.pendingRequestId != nil ? Theme.gold : Theme.secondaryText)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .topLeading)
                        if chat.favorite {
                            CIcon("star", size: 14, color: Theme.accent).padding(.top, 1)
                        }
                        if chat.muted {
                            CIcon("mute-notification", size: 14, color: Theme.mutedText).padding(.top, 1)
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

struct CallsView: View {
    var body: some View {
        VStack(spacing: 0) {
            PaneHeader(title: "Calls", background: Theme.list)
            VStack(spacing: 12) {
                IconDisc(icon: "calls", size: 44)
                Text("No calls yet.").font(.cBody).foregroundColor(Theme.secondaryText)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Theme.list)
    }
}
