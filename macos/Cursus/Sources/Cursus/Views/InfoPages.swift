import AppKit
import SwiftUI
import UniformTypeIdentifiers

// Sub-pages of a chat's profile (ChatInfoView): preferences, a member,
// invite, the group / channel link.

// MARK: - Contact preferences

/// What you allow this contact; each feature works when both of you allow it
/// ("Always" also turns it on when they only said "Yes" — SimpleX rules).
struct ContactPreferencesPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    var body: some View {
        Card(spacing: 2) {
            CardTitle(icon: "settings", title: "Chat preferences",
                      subtitle: "What you allow \(chat.title). A feature works when you both allow it.")
                .padding(.bottom, 6)
            if let prefs = chat.contactPrefs {
                ForEach(Features.contact) { feature in
                    row(feature, prefs)
                }
            } else {
                Text("Preferences are available once the contact is connected.")
                    .font(.cMeta).foregroundColor(Theme.secondaryText)
            }
        }
        .controlSize(.small)
        .labelsHidden()
    }

    @ViewBuilder
    private func row(_ feature: FeatureInfo, _ prefs: ContactPrefs) -> some View {
        let state = prefs.features[feature.key]
        let mine = state?.mine ?? "no"
        PrefRow(icon: feature.icon, label: feature.label, note: note(state)) {
            // Disappearing messages: no "always" (that is the per-chat delete
            // timer, not a negotiated preference).
            HStack(spacing: 12) {
                ChoiceMenu(options: feature.key == "timedMessages"
                            ? [Choice("no", "No"), Choice("yes", "Yes")]
                            : [Choice("no", "No"), Choice("yes", "Yes"), Choice("always", "Always")],
                           selection: mine) { v in
                    Task { await model.setContactFeature(chat, feature.key, v) }
                }
                CIcon("user-alt-1", size: 18, color: state?.contact == "yes" ? Theme.success
                      : state?.contact == "always" ? Theme.gold : Theme.danger)
                    .help("Contact: \(Features.allowLabel(state?.contact ?? "no"))")
                    .accessibilityLabel("Contact: \(Features.allowLabel(state?.contact ?? "no"))")
            }
        }
        if feature.key == "timedMessages" && mine != "no" {
            PrefRow(icon: "system-pending-line", label: "Delete after") {
                ChoiceMenu(options: ttlOptions(prefs.timedTtl), selection: prefs.timedTtl) { v in
                    Task { await model.setContactTimedTtl(chat, v) }
                }
            }
        }
    }

    private func note(_ state: ContactFeature?) -> String {
        guard let state = state else { return "" }
        let theirs = "\(chat.title): \(Features.allowLabel(state.contact).lowercased())"
        return state.enabled ? "On in this chat · \(theirs)" : "Off in this chat · \(theirs)"
    }
}

/// The preset times plus the current one if it isn't a preset.
func ttlOptions(_ current: Int64) -> [Choice<Int64>] {
    let secs = Features.timedChoices.contains(current) ? Features.timedChoices
        : (Features.timedChoices + [current]).sorted()
    return secs.map { Choice($0, Features.duration($0)) }
}

// MARK: - Group preferences

struct GroupPreferencesPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat

    private let sections = ["Messages", "Media & interaction", "Members"]

    var body: some View {
        ForEach(sections, id: \.self) { title in
            PreferenceGroup(title: title, note: title == "Messages"
                ? (chat.isOwner ? "Changes apply to everyone in this \(chat.isChannel ? "channel" : "group")."
                   : "Only owners can change these preferences.") : nil) {
                ForEach(Features.group.filter { keys(title).contains($0.key) }) { feature in
                    row(feature)
                }
                if title == "Members" && !chat.isChannel && chat.isOwner {
                    PrefRow(icon: "review-file", label: "Review new members", note: "Approve requests before people join.") {
                        Toggle("", isOn: Binding(get: { chat.memberReview },
                            set: { on in Task { await model.setMemberReview(chat, on) } }))
                            .labelsHidden()
                    }
                }
            }
            .disabled(!chat.isOwner)
            .toggleStyle(PreferenceToggleStyle())
        }
    }

    private func keys(_ title: String) -> [String] {
        switch title {
        case "Messages": return ["timedMessages", "directMessages", "fullDelete", "history"]
        case "Members": return ["support"]
        default: return ["reactions", "voice", "files", "simplexLinks"]
        }
    }

    private var prefs: GroupPrefs? { chat.groupPrefs }

    private func isOn(_ key: String) -> Bool {
        prefs?.features[key]?.on ?? (key == "support")
    }

    @ViewBuilder
    private func row(_ feature: FeatureInfo) -> some View {
        PrefRow(icon: feature.icon, label: feature.label, note: featureSummary(feature.key)) {
            Toggle("", isOn: Binding(
                get: { isOn(feature.key) },
                set: { on in Task { await model.setGroupFeature(chat, feature.key, on: on) } }))
                .toggleStyle(PreferenceToggleStyle())
                .labelsHidden()
                .controlSize(.small)
                .tint(Theme.accent)
        }
        if feature.key == "timedMessages" && isOn("timedMessages") {
            let ttl = prefs?.timedTtl ?? 86_400
            PrefRow(icon: "system-pending-line", label: "Delete after") {
                ChoiceMenu(options: ttlOptions(ttl), selection: ttl, enabled: chat.isOwner) { v in
                    Task { await model.setGroupFeature(chat, "timedMessages", on: true, ttl: v) }
                }
            }
        }
    }
}

// MARK: - Member

/// One member: message them, role, accept, hide, block, remove — each only
/// when your role allows it (same rules as the official apps).
struct MemberPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    let member: Member

    @State private var confirmRemove = false
    @State private var removeMessages = false

    /// You outrank (or equal) them: role changes, block, remove.
    private var canManage: Bool {
        !member.isMe && !member.gone && chat.roleRank >= groupRoleRank(member.role)
    }

    private var dmAllowed: Bool {
        chat.groupPrefs?.features["directMessages"]?.on ?? false
    }

    private var hasChat: Bool {
        guard let id = member.contactId else { return false }
        return model.chats.contains { $0.ref == "@\(id)" }
    }

    var body: some View {
        Card(spacing: 6) {
            HStack(spacing: 14) {
                Avatar(image: member.image, name: member.name, size: 64, kind: .contact)
                VStack(alignment: .leading, spacing: 3) {
                    Text(member.isMe ? "You" : member.name)
                        .font(.cTitle).foregroundColor(Theme.primaryText).lineLimit(2)
                        .textSelection(.enabled)
                    if !member.publicName.isEmpty && member.publicName != member.name {
                        Text(member.publicName).font(.cMeta).foregroundColor(Theme.secondaryText)
                    }
                    HStack(spacing: 4) {
                        CIcon(MemberRoles.icon(member.role), size: 14, color: Theme.secondaryText)
                        Text(statusLine).font(.cMeta).foregroundColor(Theme.secondaryText)
                    }
                }
            }
            if let bio = member.shortDescr, !bio.isEmpty {
                SectionLabel("Bio")
                ProfileText(bio).font(.cBody).foregroundColor(Theme.secondaryText)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 8)
            }
        }
        if !member.isMe && !member.gone && (hasChat || (dmAllowed && !chat.isChannel)) {
            Card(spacing: 2) {
                NavRow(icon: "paperplane", title: "Direct message",
                       subtitle: hasChat ? nil : "Starts a private chat with \(member.name)") {
                    Task { await model.messageMember(chat, member) }
                }
            }
        }
        if chat.isModerator && !member.isMe && !member.gone {
            Card(padding: 0, spacing: 0) {
                NavRow(icon: "flag-2", title: ["owner", "admin", "moderator"].contains(member.role) ? "Chat with admin" : "Chat with member",
                       flush: true) {
                    model.openSupport(chat, member: member)
                }
            }
        }
        if member.awaitingReview && chat.isModerator {
            Card(spacing: 4) {
                CardTitle(icon: "review-file", title: "Waiting for review",
                          subtitle: "\(member.name) asked to join.")
                HStack(spacing: 8) {
                    Spacer()
                    Button("Remove") { Task { await model.removeMember(chat, member, withMessages: false) } }
                        .buttonStyle(SecondaryButtonStyle())
                    Button("Accept as observer") { Task { await model.acceptMember(chat, member, role: "observer") } }
                        .buttonStyle(SecondaryButtonStyle())
                    Button("Accept") { Task { await model.acceptMember(chat, member) } }
                        .buttonStyle(PrimaryButtonStyle())
                }
            }
        }
        if !member.isMe && !member.gone {
            Card(spacing: 0) {
                if chat.isAdmin && canManage && !member.awaitingReview {
                    PrefRow(icon: MemberRoles.icon(member.role), label: "Role") {
                        ChoiceMenu(options: roleChoices.map { Choice($0, $0.capitalized) },
                                   selection: member.role) { role in
                            Task { await model.setMemberRole(chat, member, role) }
                        }
                    }
                }
                PrefRow(icon: "eye-observer", label: "Show messages",
                        note: "Off hides \(member.name)'s messages on this Mac only.") {
                    Toggle("", isOn: Binding(
                        get: { member.showMessages },
                        set: { on in Task { await model.setMemberShowMessages(chat, member, on) } }))
                        .toggleStyle(PreferenceToggleStyle())
                }
                if chat.isModerator && canManage {
                    PrefRow(icon: "block-prohibited", label: "Block for everyone",
                            note: "Members no longer see new messages from \(member.name).") {
                        Toggle("", isOn: Binding(
                            get: { member.blocked },
                            set: { on in Task { await model.blockMember(chat, member, on) } }))
                            .toggleStyle(PreferenceToggleStyle())
                    }
                }
            }
            .controlSize(.small)
            .labelsHidden()
            .tint(Theme.accent)
        }
        if chat.isAdmin && canManage {
            Card(spacing: 4) {
                if confirmRemove {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Remove \(member.name) from the group?")
                            .font(.cBodyBold).foregroundColor(Theme.primaryText)
                        Toggle("Also delete their messages for everyone", isOn: $removeMessages)
                            .toggleStyle(.checkbox)
                            .font(.cMeta)
                        HStack(spacing: 8) {
                            Spacer()
                            Button("Cancel") { confirmRemove = false }
                                .buttonStyle(SecondaryButtonStyle())
                                .keyboardShortcut(.cancelAction)
                            Button("Remove") {
                                confirmRemove = false
                                Task { await model.removeMember(chat, member, withMessages: removeMessages) }
                            }
                            .buttonStyle(DangerButtonStyle())
                        }
                    }
                    .padding(4)
                } else {
                    ActionTile(icon: "remove-profile", title: chat.isChannel ? "Remove subscriber" : "Remove member",
                               danger: true) {
                        removeMessages = false
                        confirmRemove = true
                    }
                }
            }
        }
    }

    private var roleChoices: [String] {
        var roles = MemberRoles.assignable(by: chat.role)
        if !roles.contains(member.role) { roles.append(member.role) }
        return roles
    }

    private var statusLine: String {
        if member.gone { return "\(member.role) · left" }
        if member.awaitingReview { return "\(member.role) · waiting for review" }
        if member.pending { return "\(member.role) · invited" }
        if member.blocked { return "\(member.role) · blocked" }
        return member.role
    }
}

// MARK: - Chats with members (moderators)

/// Join requests first, then members with a support thread (Linux order).
struct SupportListPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var confirmDelete: Member?

    private var rows: [Member] {
        model.members
            .filter { !$0.isMe && !$0.gone && ($0.support != nil || $0.awaitingReview) }
            .sorted { a, b in
                if a.awaitingReview != b.awaitingReview { return a.awaitingReview }
                if (a.support != nil) != (b.support != nil) { return a.support != nil }
                return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
            }
    }

    var body: some View {
        Card(spacing: 6) {
            CardTitle(icon: "flag-2", title: "Chats with members",
                      subtitle: "Private threads between a member and the moderators, and join requests.")
            if rows.isEmpty {
                Text(model.members.isEmpty ? "Loading…" : "No member chats yet.")
                    .font(.cMeta).foregroundColor(Theme.secondaryText).padding(8)
            }
            VStack(spacing: 0) {
                ForEach(rows) { member in
                    row(member)
                }
            }
        }
        if let member = confirmDelete {
            Card(spacing: 4) {
                InlineConfirm(question: "Delete the chat with \(member.name)?",
                              detail: "The thread and its messages are deleted for the moderators.",
                              confirmTitle: "Delete",
                              onCancel: { confirmDelete = nil },
                              onConfirm: {
                                  confirmDelete = nil
                                  Task { await model.deleteSupportChat(chat, member) }
                              })
            }
        }
    }

    private func row(_ member: Member) -> some View {
        Button { model.openSupport(chat, member: member, fromList: true) } label: {
            HStack(spacing: 10) {
                Avatar(image: member.image, name: member.name, size: 40, kind: .contact)
                VStack(alignment: .leading, spacing: 1) {
                    Text(member.name).font(.cBody).foregroundColor(Theme.primaryText).lineLimit(1)
                    Text(member.awaitingReview ? "Wants to join — review" : "Support chat")
                        .font(.cMeta)
                        .foregroundColor(member.awaitingReview ? Theme.gold : Theme.secondaryText)
                }
                Spacer(minLength: 6)
                if let unread = member.support?.unread, unread > 0 {
                    UnreadPill(count: unread)
                }
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundColor(Theme.mutedText)
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(GhostTileStyle())
        .contextMenu {
            if !member.awaitingReview && member.support != nil {
                Button("Delete chat…") { confirmDelete = member }
            }
        }
    }
}

// MARK: - Invite

struct InviteMembersPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var role = "member"
    @State private var invited: Set<Int64> = []
    @State private var search = ""

    private var contacts: [Chat] {
        let all = model.invitableContacts
        let q = search.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return all }
        return all.filter { $0.title.localizedCaseInsensitiveContains(q) }
    }

    var body: some View {
        Card(spacing: 6) {
            CardTitle(icon: "user-plus-alt-1", title: "Invite contacts",
                      subtitle: "They get an invitation and join when they accept.")
            PrefRow(icon: MemberRoles.icon(role), label: "Role for new members") {
                ChoiceMenu(options: MemberRoles.assignable(by: chat.role).map { Choice($0, $0.capitalized) },
                           selection: role) { r in role = r }
            }
            CursusField(placeholder: "Search contacts", text: $search)
            if contacts.isEmpty {
                Text(model.invitableContacts.isEmpty ? "All your contacts are already here." : "No contact matches.")
                    .font(.cMeta).foregroundColor(Theme.secondaryText)
                    .padding(8)
            }
            VStack(spacing: 0) {
                ForEach(contacts) { contact in
                    HStack(spacing: 10) {
                        Avatar(image: contact.image, name: contact.title, size: 36, kind: contact.avatarKind)
                        Text(contact.title).font(.cBody).foregroundColor(Theme.primaryText).lineLimit(1)
                        Spacer(minLength: 6)
                        if invited.contains(contact.apiId) {
                            Text("Invited").font(.cMeta).foregroundColor(Theme.success)
                        } else {
                            Button("Invite") {
                                invited.insert(contact.apiId)
                                Task { await model.addMember(chat, contact: contact, role: role) }
                            }
                            .buttonStyle(SecondaryButtonStyle())
                        }
                    }
                    .padding(.vertical, 4)
                    .padding(.horizontal, 6)
                }
            }
        }
    }
}

// MARK: - Group / channel link

struct GroupLinkPage: View {
    @EnvironmentObject private var model: AppModel
    let chat: Chat
    @State private var link: ConnLink?
    @State private var loading = false
    @State private var role = "member"
    @State private var confirmDelete = false

    var body: some View {
        Card(spacing: 8) {
            CardTitle(icon: "share-03", title: chat.isChannel ? "Share this channel" : "Share this group",
                      subtitle: chat.isChannel
                        ? "Anyone with this link can subscribe."
                        : "Anyone with this link can join the group.")
            if let link = link {
                LinkBox(text: link.short ?? link.display, copyText: link.short ?? link.full)
                ActionTile(icon: "share-03", title: "Share link") { Clipboard.share(link.short ?? link.full) }
            } else if loading {
                ProgressView().controlSize(.small).padding(8)
            } else if chat.isAdmin {
                HStack {
                    Spacer()
                    Button("Create link") { Task { await load() } }
                        .buttonStyle(PrimaryButtonStyle())
                }
            } else {
                Text("No link yet — ask an admin.").font(.cMeta).foregroundColor(Theme.secondaryText)
            }
            if !chat.isChannel && chat.isAdmin && link != nil {
                PrefRow(icon: MemberRoles.icon(role), label: "People joining become") {
                    ChoiceMenu(options: [Choice("member", "Member"), Choice("observer", "Observer")],
                               selection: role) { r in
                        role = r
                        Task { await model.setGroupLinkRole(chat, r) }
                    }
                }
            }
        }
        if !chat.isChannel && chat.isAdmin && link != nil {
            Card(spacing: 4) {
                if confirmDelete {
                    InlineConfirm(question: "Delete the group link?",
                                  detail: "Nobody can join with it any more. Members stay in the group.",
                                  confirmTitle: "Delete link",
                                  onCancel: { confirmDelete = false },
                                  onConfirm: {
                                      confirmDelete = false
                                      Task {
                                          await model.deleteGroupLink(chat)
                                          link = nil
                                      }
                                  })
                } else {
                    ActionTile(icon: "trash-delete", title: "Delete link", danger: true) { confirmDelete = true }
                }
            }
        }
        // Like Linux: opening the page shows the link (admins create it).
        Color.clear.frame(height: 0)
            .task { await load(quiet: !chat.isAdmin) }
    }

    private func load(quiet: Bool = false) async {
        loading = true
        link = await model.groupLink(chat, role: role, quiet: quiet)
        loading = false
    }
}


private func featureSummary(_ key: String) -> String {
    switch key {
    case "timedMessages": return "Automatically remove messages after a set time."
    case "directMessages": return "Let members connect with each other."
    case "fullDelete": return "Allow senders to remove messages for everyone."
    case "history": return "Share recent messages with new members."
    case "reactions": return "Respond with a quick emoji."
    case "voice": return "Record and share audio messages."
    case "files": return "Share photos, videos and documents."
    case "simplexLinks": return "Share SimpleX invitations in messages."
    default: return "Give members a private way to reach admins."
    }
}
