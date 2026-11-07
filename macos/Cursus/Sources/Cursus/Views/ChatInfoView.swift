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
}

/// Contact / group / channel page (⌘I). Fills the detail pane like the Linux
/// contact and group pages: the same cards, in the same order. Title, Back
/// and the favourite / mute / preferences actions sit in the native toolbar
/// where a Mac user looks for them.
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
        }
    }

    var body: some View {
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
            }
        }
        .navigationTitle(pageTitle)
        .navigationSubtitle(chat.title)
        .toolbar {
            ToolbarItem(placement: .navigation) {
                Button(action: back) { Image(systemName: "chevron.left") }
                    .help(page == .overview ? "Back to the chat" : "Back")
            }
            ToolbarItemGroup {
                if page == .overview {
                    Button { Task { await model.setFavorite(chat, !chat.favorite) } } label: {
                        CIcon("star", size: 18, color: chat.favorite ? Theme.accent : nil)
                    }
                    .help(chat.favorite ? "Remove from favorites" : "Add to favorites")
                    Button { Task { await model.setMuted(chat, !chat.muted) } } label: {
                        CIcon("mute-notification", size: 18, color: chat.muted ? Theme.accent : nil)
                    }
                    .help(chat.muted ? "Unmute" : "Mute")
                    Button { page = .preferences } label: { CIcon("preferences", size: 18) }
                        .help(pageTitleFor(.preferences))
                }
            }
        }
        .onExitCommand(perform: back)
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
                NavRow(icon: "preferences", title: "Chat preferences",
                       subtitle: "Disappearing messages, receipts, calls…", flush: true) { page = .preferences }
            }
        }
        if chat.isGroup { groupActions }
        if chat.isGroup { membersCard }
        dangerCard
            .sheet(item: $editing) { field in
                ProfileFieldSheet(chat: chat, field: field)
                    .environmentObject(model)
            }
            .onChange(of: chat.ref) { _ in confirming = nil }
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

    @ViewBuilder private var groupActions: some View {
        if ownsGroup || chat.isAdmin || chat.isChannel {
            Card(padding: 0, spacing: 0) {
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
            HStack(alignment: .firstTextBaseline, spacing: 2) {
                Text(empty ? placeholder : text)
                    .font(font)
                    .foregroundColor(empty ? Theme.mutedText : color)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                if editable {
                    IconTool(icon: "edit-3", help: help ?? "Edit", size: 14, action: edit)
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

/// Native sheet with a Cursus card inside: title, hint, field, Cancel / ✓.
struct ProfileFieldSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let chat: Chat
    let field: ProfileField

    @State private var value = ""
    @State private var error: String?

    private var meta: (title: String, hint: String, placeholder: String, multiline: Bool) {
        switch field {
        case .alias: return ("Local name", "Only you see this name.", "Local name", false)
        case .groupName: return ("Name", "Every member sees this name.", "Name", false)
        case .groupBio: return ("Short description", "Shown under the name.", "Short description", false)
        case .welcome: return ("Welcome message", "Sent to every new member when they join.",
                               "Welcome to the group! Please…", true)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(meta.title).font(.cBodyBold).foregroundColor(Theme.primaryText)
                    Text(meta.hint).font(.cMeta).foregroundColor(Theme.secondaryText)
                }
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(SecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(action: save) {
                    Image(systemName: "checkmark").font(.system(size: 13, weight: .bold))
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(meta.multiline ? KeyboardShortcut("s", modifiers: .command) : .defaultAction)
                .help("Save")
            }
            if meta.multiline {
                TextEditor(text: $value)
                    .font(.cBody)
                    .frame(minHeight: 140, maxHeight: 280)
                    .padding(6)
                    .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.fieldFill))
                    .overlay(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                        .strokeBorder(Theme.border, lineWidth: 1))
            } else {
                CursusField(placeholder: meta.placeholder, text: $value, onSubmit: save)
            }
            if let error = error {
                Text(error).font(.cMeta).foregroundColor(Theme.danger)
            }
        }
        .padding(20)
        .frame(width: 440)
        .background(Theme.elevated)
        .onAppear {
            switch field {
            case .alias: value = chat.alias ?? ""
            case .groupName: value = chat.name ?? chat.title
            case .groupBio: value = chat.shortDescr
            case .welcome: value = chat.description ?? ""
            }
        }
    }

    private func save() {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if field == .groupName && trimmed.isEmpty {
            error = "The name can't be empty."
            return
        }
        dismiss()
        Task {
            switch field {
            case .alias: await model.setAlias(chat, trimmed)
            case .groupName: await model.updateGroupProfile(chat, name: trimmed)
            case .groupBio: await model.updateGroupProfile(chat, shortDescr: String(trimmed.prefix(160)))
            case .welcome: await model.updateGroupProfile(chat, welcome: trimmed)
            }
        }
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
