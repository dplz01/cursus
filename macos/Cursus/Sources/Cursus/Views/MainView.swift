import AppKit
import SwiftUI

/// Three columns, the macOS 12 way (NavigationSplitView is macOS 13+):
/// destinations | list | detail.
struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationView {
            SidebarView()
                .frame(minWidth: 160, idealWidth: 180)
            middle
                .frame(minWidth: 260, idealWidth: 300)
            detail
                .frame(minWidth: 380)
        }
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
            Placeholder(symbol: "phone", text: "Voice calls are coming to Cursus for Mac.")
        case .chats, .channels:
            if let chat = model.selectedChat {
                ConversationView(chat: chat).id(chat.ref)
            } else {
                Placeholder(symbol: "bubble.left.and.bubble.right", text: "Select a conversation")
            }
        }
    }
}

struct Placeholder: View {
    let symbol: String
    let text: String

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 36, weight: .light))
                .foregroundColor(Theme.mutedText)
            Text(text).foregroundColor(Theme.secondaryText)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SidebarView: View {
    @EnvironmentObject private var model: AppModel

    private var selection: Binding<Destination?> {
        Binding(get: { model.destination }, set: { if let d = $0 { model.destination = d } })
    }

    var body: some View {
        List(selection: selection) {
            ForEach(Destination.allCases) { dest in
                Label(dest.title, systemImage: dest.symbol)
                    .badge(badge(for: dest))
                    .tag(dest)
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            if let user = model.user {
                HStack(spacing: 8) {
                    Avatar(image: user.image, name: user.displayName, size: 26)
                    Text(user.displayName).lineLimit(1)
                    Spacer()
                    Button { model.lock() } label: { Image(systemName: "lock") }
                        .buttonStyle(.borderless)
                        .help("Lock Cursus (⇧⌘L)")
                }
                .padding(10)
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button {
                    NSApp.keyWindow?.firstResponder?.tryToPerform(
                        #selector(NSSplitViewController.toggleSidebar(_:)), with: nil)
                } label: {
                    Image(systemName: "sidebar.left")
                }
                .help("Show or hide the sidebar")
            }
        }
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
        List(selection: $model.selection) {
            ForEach(rows) { chat in
                ChatRow(chat: chat)
                    .tag(chat.ref)
                    .contextMenu { ChatMenu(chat: chat) }
            }
        }
        .listStyle(.inset)
        .searchable(text: $search, placement: .toolbar, prompt: "Search")
        .overlay {
            if model.visibleChats.isEmpty {
                VStack(spacing: 10) {
                    Text(model.destination == .channels ? "No channels yet" : "No conversations yet")
                        .foregroundColor(Theme.secondaryText)
                    if model.destination == .chats {
                        Button("Add contact…") { model.showNewChat = true }
                    }
                }
            }
        }
        .navigationTitle(model.destination.title)
        .toolbar {
            ToolbarItem {
                Button { model.showNewChat = true } label: { Image(systemName: "square.and.pencil") }
                    .help("New chat (⌘N)")
            }
        }
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

struct ChatRow: View {
    let chat: Chat

    var body: some View {
        HStack(spacing: 10) {
            Avatar(image: chat.image, name: chat.title, size: 38, symbol: chat.isNotes ? "note.text" : nil)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    if chat.favorite {
                        Image(systemName: "star.fill").font(.caption2).foregroundColor(.yellow)
                    }
                    Text(chat.title).fontWeight(chat.hasUnread ? .semibold : .regular).lineLimit(1)
                    Spacer(minLength: 4)
                    if let date = chat.lastDate {
                        Text(Dates.listStamp(date)).font(.caption).foregroundColor(Theme.mutedText)
                    }
                }
                HStack(spacing: 4) {
                    Text(chat.pendingRequestId != nil ? "Wants to connect" : chat.preview)
                        .font(.callout)
                        .foregroundColor(Theme.secondaryText)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    if chat.muted {
                        Image(systemName: "bell.slash").font(.caption2).foregroundColor(Theme.mutedText)
                    }
                    if chat.hasUnread {
                        Text(chat.unread > 0 ? "\(chat.unread)" : " ")
                            .font(.caption2.weight(.semibold))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .frame(minWidth: 18)
                            .background(Capsule().fill(chat.muted ? Theme.mutedText : Theme.accent))
                            .foregroundColor(.white)
                    }
                }
            }
        }
        .padding(.vertical, 4)
    }
}

struct Avatar: View {
    let image: String?
    let name: String
    var size: CGFloat = 36
    var symbol: String? = nil

    var body: some View {
        Group {
            if let nsImage = Images.fromDataURL(image) {
                Image(nsImage: nsImage).resizable().scaledToFill()
            } else if let symbol = symbol {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.45))
                    .foregroundColor(.white)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Theme.accent)
            } else {
                Text(initials)
                    .font(.system(size: size * 0.4, weight: .semibold))
                    .foregroundColor(.white)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(color)
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }

    private var initials: String {
        let parts = name.split(separator: " ").prefix(2)
        let letters = parts.compactMap { $0.first }.map { String($0) }.joined()
        return letters.isEmpty ? "?" : letters.uppercased()
    }

    private var color: Color {
        let palette: [Color] = [.blue, .purple, .pink, .orange, .teal, .green, .indigo, .brown]
        let hash = name.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0x7fffffff }
        return palette[hash % palette.count]
    }
}

struct CallsView: View {
    var body: some View {
        Placeholder(symbol: "phone", text: "No calls yet")
            .navigationTitle("Calls")
    }
}
