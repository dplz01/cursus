import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Pages of a chat's profile (Back returns to the overview, then to the chat).
enum InfoPage: Equatable {
    case overview
    case preferences
    case member(Int64)
    case invite
    case link
    /// Moderators: members' support threads and join requests.
    case supportList
}

/// Contact / group / channel page (⌘I). Fills the detail pane like the Linux
/// contact and group pages: header with Back (Esc), title and the favourite /
/// mute / preferences actions, then the same cards in the same order.
struct ChatInfoView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    @State private var page: InfoPage = .overview

    private var kindTitle: String {
        if chat.isChannel { return "Channel" }
        if chat.isGroup { return "Group" }
        return "Contact"
    }

    private var pageTitle: String {
        switch page {
        case .overview: return kindTitle
        case .preferences: return chat.isDirect ? "Chat preferences" : "\(kindTitle) preferences"
        case .member: return chat.isChannel ? "Subscriber" : "Member"
        case .invite: return "Invite members"
        case .link: return "\(kindTitle) link"
        case .supportList: return "Chats with members"
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            PaneHeader(title: pageTitle, back: back) {
                if page == .overview {
                    IconTool(icon: chat.favorite ? "star-filled" : "star", help: chat.favorite ? "Remove from favorites" : "Add to favorites",
                             active: chat.favorite, size: 18) {
                        Task { await model.setFavorite(chat, !chat.favorite) }
                    }
                    IconTool(icon: chat.muted ? "notifications-off" : "notifications-off-outline", help: chat.muted ? "Unmute" : "Mute",
                             active: chat.muted, size: 18) {
                        Task { await model.setMuted(chat, !chat.muted) }
                    }
                    IconTool(icon: "settings", help: pageTitleFor(.preferences), size: 18) {
                        page = .preferences
                    }
                }
            }
            pages
        }
        .background(Theme.panel)
        .onAppear {
            // Back from a member's thread opened from the list: the list again.
            if let returning = model.infoReturnPage {
                page = returning
                model.infoReturnPage = nil
            }
        }
    }

    private var pages: some View {
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
                ChatSettingsCard(chat: chat)
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
            case .supportList:
                SupportListPage(chat: chat)
            }
        }
    }

    private func pageTitleFor(_ p: InfoPage) -> String {
        p == .preferences ? (chat.isDirect ? "Chat preferences" : "\(kindTitle) preferences") : kindTitle
    }

    private func back() {
        if page == .overview {
            model.showInfo = false
        } else {
            page = .overview
        }
    }
}

// MARK: - Overview

/// Which profile text the edit sheet is changing.
enum ProfileField: String, Identifiable {
    case alias, groupName, groupBio, welcome
    var id: String { rawValue }
}

private struct InfoOverview: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @Binding var page: InfoPage

    @State private var editing: ProfileField?
    @State private var confirming: Danger?

    private enum Danger { case clear, leave, delete }

    private var publicName: String {
        let name = chat.name ?? ""
        return name.isEmpty ? chat.title : name
    }

    private var amMember: Bool { model.members.contains { $0.isMe && !$0.gone } }
    private var ownsGroup: Bool { chat.isGroup && chat.isOwner }

    var body: some View {
        identity
        if chat.isDirect, let link = chat.contactLink, !link.isEmpty {
            Card {
                CardTitle(icon: "link-minimalistic-2", title: "Address",
                          subtitle: "\(publicName) shares this link on their profile.")
                LinkBox(text: link)
            }
        }
        if chat.isDirect && chat.pendingRequestId == nil {
            Card(padding: 0, spacing: 0) {
                NavRow(icon: "settings", title: "Chat preferences",
                       subtitle: "Disappearing messages, receipts, calls…", flush: true) { page = .preferences }
            }
        }
        if chat.isGroup { groupActions }
        if chat.isGroup { membersCard }
        dangerCard
            .sheet(item: $editing) { field in editor(for: field) }
            .onChange(of: chat.ref) { _ in confirming = nil }
    }

    // MARK: Edit sheets

    private func editor(for field: ProfileField) -> TextEditSheet {
        switch field {
        case .alias:
            return TextEditSheet(title: "Local name", hint: "Only you see this name.", placeholder: "Local name",
                                 initial: chat.alias ?? "") { v in Task { await model.setAlias(chat, v) } }
        case .groupName:
            return TextEditSheet(title: "Name", hint: "Every member sees this name.", placeholder: "Name",
                                 required: true, initial: chat.name ?? chat.title) { v in
                Task { await model.updateGroupProfile(chat, name: v) }
            }
        case .groupBio:
            return TextEditSheet(title: "Short description", hint: "Shown under the name.",
                                 placeholder: "Short description", initial: chat.shortDescr) { v in
                Task { await model.updateGroupProfile(chat, shortDescr: String(v.prefix(160))) }
            }
        case .welcome:
            return TextEditSheet(title: "Welcome message", hint: "Sent to every new member when they join.",
                                 placeholder: "Welcome to the group! Please…", multiline: true,
                                 initial: chat.description ?? "") { v in
                Task { await model.updateGroupProfile(chat, welcome: v) }
            }
        }
    }

    // MARK: Profile card

    private var identity: some View {
        Card(spacing: 6) {
            HStack(alignment: .center, spacing: 14) {
                VStack(spacing: 8) {
                    Avatar(image: chat.image, name: chat.title, size: 72, kind: chat.avatarKind)
                    if ownsGroup { photoActions }
                }
                VStack(alignment: .leading, spacing: 4) {
                    EditableLine(text: publicName, placeholder: "Name", style: .name,
                                 editable: ownsGroup) { editing = .groupName }
                    if chat.pendingRequestId == nil {
                        EditableLine(text: chat.alias ?? "", placeholder: "Add a local name", style: .sub,
                                     editable: true, help: "Only you see this name") { editing = .alias }
                    }
                    if !chat.fullName.isEmpty && chat.fullName != publicName {
                        Text(chat.fullName).font(.cMeta).foregroundColor(Theme.secondaryText)
                            .padding(.leading, 13)
                    }
                    if chat.isGroup, let role = chat.role {
                        Text("You are \(role)").font(.cMeta).foregroundColor(Theme.secondaryText)
                            .padding(.leading, 13)
                    }
                }
            }
            if ownsGroup {
                SectionLabel("Short description")
                EditableLine(text: chat.shortDescr, placeholder: "Add a short description", style: .body,
                             editable: true) { editing = .groupBio }
            } else {
                if !chat.shortDescr.isEmpty {
                    SectionLabel(chat.isGroup ? "About" : "Bio")
                    about(chat.shortDescr)
                }
                if let text = chat.description, !text.isEmpty {
                    SectionLabel(chat.isGroup ? "Welcome message" : "About")
                    about(text)
                }
            }
        }
    }

    private var photoActions: some View {
        HStack(spacing: 6) {
            IconTool(icon: "image-pen", help: "Choose picture…", size: 18, action: pickImage)
            if chat.image != nil {
                IconTool(icon: "cancel", help: "Remove picture", size: 18) {
                    Task { await model.updateGroupProfile(chat, image: .some(nil)) }
                }
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

    private func pickImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url, let picked = NSImage(contentsOf: url) else { return }
        guard let dataURL = Images.profileDataURL(from: picked) else {
            model.errorMessage = "That picture couldn't be used."
            return
        }
        Task { await model.updateGroupProfile(chat, image: .some(dataURL)) }
    }

    // MARK: Welcome / invite / link

    /// Member support is on unless the owners switched it off.
    private var supportOn: Bool { chat.groupPrefs?.features["support"]?.on ?? true }

    @ViewBuilder private var groupActions: some View {
        if ownsGroup || chat.isAdmin || chat.isChannel || chat.isModerator || supportOn {
            Card(padding: 0, spacing: 0) {
                if chat.isModerator {
                    NavRow(icon: "admin", title: "Chats with members",
                           subtitle: chat.membersRequireAttention > 0
                            ? "\(chat.membersRequireAttention) need attention"
                            : "Join requests and member support",
                           flush: true) { page = .supportList }
                } else if supportOn && chat.role != nil {
                    NavRow(icon: "admin", title: "Chat with admins",
                           subtitle: chat.supportUnread > 0
                            ? "\(chat.supportUnread) unread"
                            : "Private conversation with moderators",
                           flush: true) { model.openSupport(chat) }
                }
                if ownsGroup {
                    NavRow(icon: "scripture-scroll", title: "Welcome message", subtitle: welcomePreview,
                           trailing: "edit-3", flush: true) { editing = .welcome }
                }
                if chat.isAdmin && !chat.isChannel {
                    NavRow(icon: "user-plus-alt-1", title: "Invite", subtitle: "Your contacts",
                           trailing: "plus", flush: true) { page = .invite }
                }
                if chat.isAdmin || chat.isChannel {
                    NavRow(icon: "link-minimalistic-2", title: "\(chat.isChannel ? "Channel" : "Group") link",
                           subtitle: chat.isAdmin ? "Anyone with it can join" : "Share this channel",
                           flush: true) { page = .link }
                }
            }
        }
    }

    private var welcomePreview: String {
        let text = (chat.description ?? "").split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return text.isEmpty ? "Greet the members who join" : text
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

    @ViewBuilder private var dangerCard: some View {
        if let confirming = confirming {
            Card(spacing: 4) { confirm(confirming) }
        } else {
            Card(padding: 0, spacing: 0) {
                HStack(spacing: 0) {
                    ActionTile(icon: "clear-all", title: "Clear chat", stacked: true) { confirming = .clear }
                    if chat.isGroup && amMember {
                        ActionTile(icon: "session-leave", title: chat.isChannel ? "Leave channel" : "Leave group",
                                   stacked: true) { confirming = .leave }
                    }
                    ActionTile(icon: "trash-delete", title: deleteTitle, danger: true, stacked: true) {
                        confirming = .delete
                    }
                }
            }
        }
    }

    @ViewBuilder private func confirm(_ danger: Danger) -> some View {
        switch danger {
        case .clear:
            InlineConfirm(question: "Clear all messages?",
                          detail: "Messages are deleted from this Mac only.",
                          confirmTitle: "Clear",
                          onCancel: { confirming = nil },
                          onConfirm: {
                              confirming = nil
                              Task { await model.clear(chat) }
                          })
        case .leave:
            InlineConfirm(question: chat.isChannel ? "Leave this channel?" : "Leave this group?",
                          detail: "You stop receiving messages. The chat stays until you delete it.",
                          confirmTitle: "Leave",
                          onCancel: { confirming = nil },
                          onConfirm: {
                              confirming = nil
                              Task { await model.leaveGroup(chat) }
                          })
        case .delete:
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
        }
    }

    private var deleteTitle: String {
        if chat.isChannel { return "Delete channel" }
        if chat.isGroup { return "Delete group" }
        return "Delete contact"
    }
}

// MARK: - Editable profile text

/// Profile text shown as plain text, with a pencil right after it that opens
/// the edit sheet (Linux: the same line and pencil). Empty: a muted hint.
struct EditableLine: View {
    enum Style { case name, sub, body }

    let text: String
    let placeholder: String
    let style: Style
    var editable = true
    var help: String? = nil
    let edit: () -> Void

    private var empty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        if editable || !empty {
            HStack(alignment: .center, spacing: 2) {
                Text(empty ? placeholder : text)
                    .font(font)
                    .foregroundColor(empty ? Theme.mutedText : color)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                if editable {
                    PencilButton(help: help ?? "Edit", action: edit)
                }
                Spacer(minLength: 0)
            }
            .padding(.leading, style == .body ? 8 : 13)
        }
    }

    private var font: Font {
        switch style {
        case .name: return .cTitle
        case .sub, .body: return .cBody
        }
    }

    private var color: Color {
        style == .name ? Theme.primaryText : Theme.secondaryText
    }
}

/// Native sheet with a Cursus card inside: title, hint, field, ✕ / ✓
/// (Linux: the editor sheet). Used for every editable profile text.
struct TextEditSheet: View {
    @Environment(\.dismiss) private var dismiss
    let title: String
    let hint: String
    let placeholder: String
    var multiline = false
    /// Empty can't be saved (names).
    var required = false
    let initial: String
    let save: (String) -> Void

    @State private var value = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.cBodyBold).foregroundColor(Theme.primaryText)
                    Text(hint).font(.cMeta).foregroundColor(Theme.secondaryText)
                }
                Spacer()
                IconTool(icon: "cancel", help: "Cancel (Esc)", size: 16) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(action: commit) {
                    Image(systemName: "checkmark").font(.system(size: 13, weight: .bold))
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(multiline ? KeyboardShortcut("s", modifiers: .command) : .defaultAction)
                .help(multiline ? "Save (⌘S)" : "Save (Return)")
            }
            if multiline {
                TextEditor(text: $value)
                    .font(.cBody)
                    .frame(minHeight: 140, maxHeight: 280)
                    .padding(6)
                    .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.fieldFill))
                    .overlay(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                        .strokeBorder(Theme.border, lineWidth: 1))
            } else {
                CursusField(placeholder: placeholder, text: $value, onSubmit: commit)
            }
            if let error = error {
                Text(error).font(.cMeta).foregroundColor(Theme.danger)
            }
        }
        .padding(20)
        .frame(width: 440)
        .background(Theme.elevated)
        .onAppear { value = initial }
    }

    private func commit() {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if required && trimmed.isEmpty {
            error = "The name can't be empty."
            return
        }
        dismiss()
        save(trimmed)
    }
}

// MARK: - Your settings for this chat

/// Notifications, receipts, auto-delete and lists — only for you (shown on
/// the preferences page, as on Linux).
struct ChatSettingsCard: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var newList = ""
    @State private var addingList = false

    var body: some View {
        Card(spacing: 0) {
            CardTitle(icon: "user-alt-1", title: "Your settings",
                      subtitle: "Only for you — \(chat.isDirect ? "they don't" : "other members don't") see these.")
                .padding(.bottom, 6)
            PrefRow(icon: "notifications", label: "Notifications") {
                ChoiceMenu(options: notificationChoices, selection: chat.notifications) { mode in
                    Task { await model.setNotifications(chat, mode) }
                }
            }
            PrefRow(icon: "check-read", label: "Send receipts",
                    note: chat.isGroup ? "Only in groups of up to 20 members." : nil) {
                ChoiceMenu(options: [Choice(-1, "Default (\(defaultReceipts ? "on" : "off"))"),
                                     Choice(1, "On"), Choice(0, "Off")],
                           selection: chat.sendReceipts.map { $0 ? 1 : 0 } ?? -1) { v in
                    Task { await model.setSendReceipts(chat, v < 0 ? nil : v == 1) }
                }
            }
            PrefRow(icon: "history", label: "Delete messages after",
                    note: "On this Mac only — others keep theirs.") {
                ChoiceMenu(options: deleteAfterChoices, selection: chat.chatItemTtl ?? 0) { v in
                    Task { await model.setChatTtl(chat, v == 0 ? nil : v) }
                }
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
                    CursusField(placeholder: "List name", text: $newList, onSubmit: createList)
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

    private var notificationChoices: [Choice<String>] {
        var out = [Choice("all", "All messages")]
        if chat.isGroup { out.append(Choice("mentions", "Mentions only")) }
        out.append(Choice("none", "Muted"))
        return out
    }

    private var deleteAfterChoices: [Choice<Int64>] {
        var secs = Features.deleteAfterChoices.compactMap { $0 }
        if let custom = chat.chatItemTtl, !secs.contains(custom) { secs.append(custom) }
        return [Choice(Int64(0), "Never")] + secs.sorted().map { Choice($0, Features.duration($0)) }
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
            .contentShape(Rectangle())
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
