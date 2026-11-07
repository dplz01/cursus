import SwiftUI

/// Contact / group / channel page, shown as an inspector beside the
/// conversation (⌘I, the toolbar's info button, or the chat's context menu).
/// Same content as the Linux contact and group pages: profile header,
/// favourite / mute, address or members, then clear / leave / delete.
struct ChatInfoView: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    @State private var alias = ""
    @State private var confirming: Danger?

    private enum Danger { case clear, leave, delete }

    private var kindTitle: String {
        if chat.isNotes { return "Notes" }
        if chat.isChannel { return "Channel" }
        if chat.isGroup { return "Group" }
        return "Contact"
    }

    private var publicName: String {
        let name = chat.name ?? ""
        return name.isEmpty ? chat.title : name
    }

    private var amMember: Bool { model.members.contains { $0.isMe } }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(kindTitle).font(.cTitle).foregroundColor(Theme.primaryText)
                Spacer()
                IconTool(icon: "cancel", help: "Close (⌘I)", size: 16) { model.showInfo = false }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            Hairline()
            Page {
                identity
                if !chat.isNotes { settingsCard }
                if !chat.isGroup && !chat.isNotes, let link = chat.contactLink, !link.isEmpty {
                    Card {
                        CardTitle(icon: "link-minimalistic-2", title: "Address",
                                  subtitle: "\(publicName) shares this link on their profile.")
                        LinkBox(text: link)
                    }
                }
                if chat.isGroup { membersCard }
                dangerCard
            }
        }
        .background(Theme.panel)
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
                    Text(chat.isNotes ? "Private notes" : publicName)
                        .font(.cTitle)
                        .foregroundColor(Theme.primaryText)
                        .lineLimit(2)
                        .textSelection(.enabled)
                        .padding(.horizontal, 13)
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
                SectionLabel(chat.isGroup ? "Description" : "About")
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

    // MARK: Favourite / mute

    private var settingsCard: some View {
        Card(spacing: 0) {
            PrefRow(icon: "star", label: "Favorite") {
                Toggle("", isOn: Binding(
                    get: { chat.favorite },
                    set: { on in Task { await model.setFavorite(chat, on) } }))
            }
            PrefRow(icon: "mute-notification", label: "Mute notifications") {
                Toggle("", isOn: Binding(
                    get: { chat.muted },
                    set: { on in Task { await model.setMuted(chat, on) } }))
            }
        }
        .toggleStyle(.switch)
        .controlSize(.small)
        .labelsHidden()
        .tint(Theme.accent)
    }

    // MARK: Members

    private var membersCard: some View {
        Card(spacing: 6) {
            CardTitle(icon: "user-group", title: chat.isChannel ? "Subscribers" : "Members",
                      subtitle: model.members.isEmpty ? "Loading…" : memberCount)
            VStack(spacing: 0) {
                ForEach(model.members) { member in
                    MemberRow(member: member)
                }
            }
        }
    }

    private var memberCount: String {
        let n = model.members.count
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

/// 40px avatar · name (yourself: “YOU (name)”) + bio · pending clock · role.
private struct MemberRow: View {
    @EnvironmentObject private var model: AppModel
    let member: Member

    private var opensChat: Bool {
        guard let id = member.contactId, !member.isMe else { return false }
        return model.chats.contains { $0.ref == "@\(id)" }
    }

    var body: some View {
        Button {
            if let id = member.contactId { model.openContact(id) }
        } label: {
            HStack(spacing: 10) {
                Avatar(image: member.image, name: member.name, size: 40, kind: .contact)
                VStack(alignment: .leading, spacing: 1) {
                    HStack(spacing: 4) {
                        Text(member.isMe ? "YOU (\(member.publicName.isEmpty ? member.name : member.publicName))" : member.name)
                            .font(.cBody)
                            .foregroundColor(member.blocked ? Theme.mutedText : Theme.primaryText)
                            .lineLimit(1)
                            .layoutPriority(-1)
                        if member.pending {
                            CIcon("system-pending-line", size: 16, color: Theme.gold)
                                .help("Invited — not connected yet")
                        }
                    }
                    if let bio = member.shortDescr, !bio.isEmpty {
                        Text(bio).font(.cMeta).foregroundColor(Theme.secondaryText).lineLimit(1)
                    }
                }
                Spacer(minLength: 6)
                HStack(spacing: 4) {
                    CIcon(roleIcon, size: 16, color: Theme.secondaryText)
                    Text(member.role).font(.cMeta).foregroundColor(Theme.secondaryText)
                }
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
        }
        .buttonStyle(GhostTileStyle())
        .disabled(!opensChat)
        .help(opensChat ? "Open chat with \(member.name)" : "")
    }

    private var roleIcon: String {
        switch member.role {
        case "owner": return "owner"
        case "admin", "moderator": return "admin"
        case "observer": return "eye-observer"
        default: return "user-member"
        }
    }
}
