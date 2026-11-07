import AppKit
import SwiftUI

/// Pages inside the info panel (back arrow returns to the overview).
enum InfoPage: Equatable {
    case overview
    case preferences
    case member(Int64)
    case invite
    case link
    case editProfile
}

/// Contact / group / channel page, shown as an inspector beside the
/// conversation (⌘I, the toolbar's info button, or the chat's context menu).
/// Same content as the Linux contact and group pages: profile header,
/// your settings for this chat, preferences, members, link, then clear /
/// leave / delete.
struct ChatInfoView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    @State private var page: InfoPage = .overview

    private var kindTitle: String {
        if chat.isNotes { return "Notes" }
        if chat.isChannel { return "Channel" }
        if chat.isGroup { return "Group" }
        return "Contact"
    }

    private var pageTitle: String {
        switch page {
        case .overview: return kindTitle
        case .preferences: return chat.isGroup ? (chat.isChannel ? "Channel preferences" : "Group preferences") : "Chat preferences"
        case .member: return chat.isChannel ? "Subscriber" : "Member"
        case .invite: return "Invite members"
        case .link: return chat.isChannel ? "Channel link" : "Group link"
        case .editProfile: return chat.isChannel ? "Edit channel" : "Edit group"
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                if page != .overview {
                    IconTool(icon: "back", help: "Back", size: 16) { page = .overview }
                }
                Text(pageTitle).font(.cTitle).foregroundColor(Theme.primaryText).lineLimit(1)
                Spacer()
                IconTool(icon: "cancel", help: "Close (⌘I)", size: 16) { model.showInfo = false }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            Hairline()
            Page {
                switch page {
                case .overview:
                    InfoOverview(chat: chat, page: $page)
                case .preferences:
                    if chat.isGroup {
                        GroupPreferencesPage(chat: chat)
                    } else {
                        ContactPreferencesPage(chat: chat)
                    }
                case .member(let id):
                    if let member = model.members.first(where: { $0.id == id }) {
                        MemberPage(chat: chat, member: member)
                    } else {
                        Text("This member is no longer in the group.")
                            .font(.cBody).foregroundColor(Theme.secondaryText)
                    }
                case .invite:
                    InviteMembersPage(chat: chat)
                case .link:
                    GroupLinkPage(chat: chat)
                case .editProfile:
                    EditGroupPage(chat: chat) { page = .overview }
                }
            }
        }
        .background(Theme.panel)
        .onChange(of: chat.ref) { _ in page = .overview }
    }
}

// MARK: - Overview

private struct InfoOverview: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @Binding var page: InfoPage

    @State private var alias = ""
    @State private var confirming: Danger?

    private enum Danger { case clear, leave, delete }

    private var publicName: String {
        let name = chat.name ?? ""
        return name.isEmpty ? chat.title : name
    }

    private var amMember: Bool { model.members.contains { $0.isMe && !$0.gone } }

    var body: some View {
        identity
        if !chat.isNotes && chat.pendingRequestId == nil {
            ChatSettingsCard(chat: chat)
            featuresCard
        }
        if chat.isDirect, let link = chat.contactLink, !link.isEmpty {
            Card {
                CardTitle(icon: "link-minimalistic-2", title: "Address",
                          subtitle: "\(publicName) shares this link on their profile.")
                LinkBox(text: link)
            }
        }
        if chat.isGroup { membersCard }
        dangerCard
            .onAppear { alias = chat.alias ?? "" }
            .onChange(of: chat.ref) { _ in
                alias = chat.alias ?? ""
                confirming = nil
            }
            .onChange(of: chat.alias) { new in alias = new ?? "" }
    }

    // MARK: Profile header

    private var identity: some View {
        Card(spacing: 6) {
            HStack(alignment: .center, spacing: 14) {
                Avatar(image: chat.image, name: chat.title, size: 72, kind: chat.avatarKind)
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 2) {
                        Text(chat.isNotes ? "Private notes" : publicName)
                            .font(.cTitle)
                            .foregroundColor(Theme.primaryText)
                            .lineLimit(2)
                            .textSelection(.enabled)
                        if chat.isGroup && chat.isOwner {
                            IconTool(icon: "edit-3", help: "Edit name, bio, welcome message and picture", size: 15) {
                                page = .editProfile
                            }
                        }
                    }
                    .padding(.leading, 13)
                    if !chat.isNotes && chat.pendingRequestId == nil {
                        RoundedField(placeholder: "Local name", text: $alias) {
                            Task { await model.setAlias(chat, alias) }
                        }
                        .help("Only you see this name")
                    }
                    if !chat.fullName.isEmpty && chat.fullName != publicName {
                        Text(chat.fullName)
                            .font(.cMeta)
                            .foregroundColor(Theme.secondaryText)
                            .padding(.horizontal, 13)
                    }
                    if chat.isGroup, let role = chat.role {
                        Text("You are \(role)")
                            .font(.cMeta)
                            .foregroundColor(Theme.secondaryText)
                            .padding(.horizontal, 13)
                    }
                }
            }
            if !chat.shortDescr.isEmpty {
                SectionLabel("Bio")
                about(chat.shortDescr)
            }
            if let description = chat.description, !description.isEmpty {
                SectionLabel(chat.isGroup ? "Welcome message" : "About")
                about(description)
            }
        }
    }

    private func about(_ text: String) -> some View {
        Text(text)
            .font(.cBody)
            .foregroundColor(Theme.secondaryText)
            .textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 8)
    }

    // MARK: Preferences / invite / link

    @ViewBuilder private var featuresCard: some View {
        Card(spacing: 2) {
            if chat.isDirect {
                NavRow(icon: "preferences", title: "Chat preferences",
                       subtitle: "Disappearing messages, reactions, calls…") { page = .preferences }
            }
            if chat.isGroup {
                NavRow(icon: "preferences", title: chat.isChannel ? "Channel preferences" : "Group preferences",
                       subtitle: chat.isOwner ? "What members can do" : "What members can do (view only)") {
                    page = .preferences
                }
                if chat.isAdmin && !chat.isChannel {
                    NavRow(icon: "user-plus-alt-1", title: "Invite members", subtitle: "Add your contacts") {
                        page = .invite
                    }
                }
                if chat.isAdmin || chat.isChannel {
                    NavRow(icon: "link-minimalistic-2", title: chat.isChannel ? "Channel link" : "Group link",
                           subtitle: chat.isAdmin ? "Anyone with it can join" : "Share this channel") {
                        page = .link
                    }
                }
            }
        }
    }

    // MARK: Members

    private var membersCard: some View {
        Card(spacing: 6) {
            CardTitle(icon: "user-group", title: chat.isChannel ? "Subscribers" : "Members",
                      subtitle: model.members.isEmpty ? "Loading…" : memberCount)
            VStack(spacing: 0) {
                ForEach(model.members.filter { !$0.gone }) { member in
                    MemberRow(member: member) { page = .member(member.id) }
                }
            }
        }
    }

    private var memberCount: String {
        let n = model.members.filter { !$0.gone }.count
        return n == 1 ? "1 member" : "\(n) members"
    }

    // MARK: Clear / leave / delete

    private var dangerCard: some View {
        Card(spacing: 4) {
            switch confirming {
            case .clear?:
                InlineConfirm(question: "Clear all messages?",
                              detail: "Messages are deleted from this Mac only.",
                              confirmTitle: "Clear",
                              onCancel: { confirming = nil },
                              onConfirm: {
                                  confirming = nil
                                  Task { await model.clear(chat) }
                              })
            case .leave?:
                InlineConfirm(question: chat.isChannel ? "Leave this channel?" : "Leave this group?",
                              detail: "You stop receiving messages. The chat stays until you delete it.",
                              confirmTitle: "Leave",
                              onCancel: { confirming = nil },
                              onConfirm: {
                                  confirming = nil
                                  Task { await model.leaveGroup(chat) }
                              })
            case .delete?:
                InlineConfirm(question: "Delete \(chat.title)?",
                              detail: chat.isGroup
                                ? "The chat and its messages are deleted from this Mac."
                                : "The contact, the connection and all messages are deleted.",
                              confirmTitle: "Delete",
                              onCancel: { confirming = nil },
                              onConfirm: {
                                  confirming = nil
                                  Task { await model.delete(chat) }
                              })
            case nil:
                ActionTile(icon: "clear-all", title: "Clear chat") { confirming = .clear }
                if chat.isGroup && amMember {
                    ActionTile(icon: "session-leave", title: chat.isChannel ? "Leave channel" : "Leave group") {
                        confirming = .leave
                    }
                }
                if !chat.isNotes {
                    ActionTile(icon: "trash-delete", title: deleteTitle, danger: true) { confirming = .delete }
                }
            }
        }
    }

    private var deleteTitle: String {
        if chat.isChannel { return "Delete channel" }
        if chat.isGroup { return "Delete group" }
        return "Delete contact"
    }
}

// MARK: - Your settings for this chat

/// Favourite, notifications, receipts, auto-delete and lists — only for you.
private struct ChatSettingsCard: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var newList = ""
    @State private var addingList = false

    var body: some View {
        Card(spacing: 0) {
            PrefRow(icon: "star", label: "Favorite") {
                Toggle("", isOn: Binding(
                    get: { chat.favorite },
                    set: { on in Task { await model.setFavorite(chat, on) } }))
                    .toggleStyle(.switch)
            }
            PrefRow(icon: "mute-notification", label: "Notifications") {
                Picker("", selection: Binding(
                    get: { chat.notifications },
                    set: { mode in Task { await model.setNotifications(chat, mode) } })) {
                    Text("All messages").tag("all")
                    if chat.isGroup { Text("Mentions only").tag("mentions") }
                    Text("Muted").tag("none")
                }
                .pickerStyle(.menu)
                .fixedSize()
            }
            PrefRow(icon: "check-read", label: "Send receipts",
                    note: chat.isGroup ? "Only in groups of up to 20 members." : nil) {
                Picker("", selection: Binding<Int>(
                    get: { chat.sendReceipts.map { $0 ? 1 : 0 } ?? -1 },
                    set: { v in Task { await model.setSendReceipts(chat, v < 0 ? nil : v == 1) } })) {
                    Text("Default (\(defaultReceipts ? "on" : "off"))").tag(-1)
                    Text("On").tag(1)
                    Text("Off").tag(0)
                }
                .pickerStyle(.menu)
                .fixedSize()
            }
            PrefRow(icon: "history", label: "Delete messages after",
                    note: "On this Mac only — others keep theirs.") {
                Picker("", selection: Binding<Int64>(
                    get: { chat.chatItemTtl ?? 0 },
                    set: { v in Task { await model.setChatTtl(chat, v == 0 ? nil : v) } })) {
                    Text("Never").tag(Int64(0))
                    ForEach(Features.deleteAfterChoices.compactMap { $0 }, id: \.self) { secs in
                        Text(Features.duration(secs)).tag(secs)
                    }
                    if let custom = chat.chatItemTtl, !Features.deleteAfterChoices.contains(custom) {
                        Text(Features.duration(custom)).tag(custom)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
            }
            PrefRow(icon: "select-multiple", label: "Lists", note: listsNote) {
                Menu {
                    ForEach(model.chatTags) { tag in
                        Button {
                            Task { await model.toggleTag(chat, tag) }
                        } label: {
                            if chat.tagIds.contains(tag.id) {
                                Label(tagTitle(tag), systemImage: "checkmark")
                            } else {
                                Text(tagTitle(tag))
                            }
                        }
                    }
                    if !model.chatTags.isEmpty { Divider() }
                    Button("New list…") { addingList = true }
                } label: {
                    Text(chat.tagIds.isEmpty ? "Add" : "Edit")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            if addingList {
                HStack(spacing: 6) {
                    CursusField(placeholder: "List name", text: $newList) { createList() }
                    Button("Add", action: createList)
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(newList.trimmingCharacters(in: .whitespaces).isEmpty)
                    IconTool(icon: "cancel", help: "Cancel", size: 14) {
                        addingList = false
                        newList = ""
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
            }
        }
        .controlSize(.small)
        .labelsHidden()
        .tint(Theme.accent)
        .task { await model.loadChatTags() }
    }

    private var defaultReceipts: Bool {
        guard let user = model.user else { return true }
        return chat.isGroup ? user.sendRcptsSmallGroups : user.sendRcptsContacts
    }

    private var listsNote: String? {
        let names = model.chatTags.filter { chat.tagIds.contains($0.id) }.map(tagTitle)
        return names.isEmpty ? nil : names.joined(separator: ", ")
    }

    private func tagTitle(_ tag: ChatTag) -> String {
        if let emoji = tag.emoji, !emoji.isEmpty { return "\(emoji) \(tag.text)" }
        return tag.text
    }

    private func createList() {
        let name = newList
        newList = ""
        addingList = false
        Task { await model.createChatTag(name, adding: chat) }
    }
}

// MARK: - Member row

/// 40px avatar · name (yourself: “YOU (name)”) + bio · pending clock · role.
private struct MemberRow: View {
    let member: Member
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 10) {
                Avatar(image: member.image, name: member.name, size: 40, kind: .contact)
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 4) {
                        Text(member.isMe ? "YOU (\(member.publicName.isEmpty ? member.name : member.publicName))" : member.name)
                            .font(.cBody)
                            .foregroundColor(member.blocked || !member.showMessages ? Theme.mutedText : Theme.primaryText)
                            .lineLimit(1)
                            .layoutPriority(-1)
                        if member.pending {
                            CIcon("system-pending-line", size: 16, color: Theme.gold)
                                .help(member.awaitingReview ? "Waiting for review" : "Invited — not connected yet")
                        }
                        if member.blocked {
                            CIcon("block-prohibited", size: 14, color: Theme.danger).help("Blocked by an admin")
                        }
                    }
                    if let bio = member.shortDescr, !bio.isEmpty {
                        Text(bio).font(.cMeta).foregroundColor(Theme.secondaryText).lineLimit(1)
                    }
                }
                Spacer(minLength: 6)
                HStack(spacing: 4) {
                    CIcon(MemberRoles.icon(member.role), size: 16, color: Theme.secondaryText)
                    Text(member.role).font(.cMeta).foregroundColor(Theme.secondaryText)
                }
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
        }
        .buttonStyle(GhostTileStyle())
        .help("Member details")
    }
}

enum MemberRoles {
    static let all = ["observer", "member", "moderator", "admin", "owner"]

    static func icon(_ role: String) -> String {
        switch role {
        case "owner": return "owner"
        case "admin", "moderator": return "admin"
        case "observer": return "eye-observer"
        default: return "user-member"
        }
    }

    /// Roles you may give: up to your own.
    static func assignable(by myRole: String?) -> [String] {
        let mine = myRole.map(groupRoleRank) ?? 0
        return all.filter { groupRoleRank($0) <= mine }
    }
}
