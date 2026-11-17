import SwiftUI

/// Yellow "needs attention" square with a round count on its top-right
/// corner (Linux `attention_button`): pending links, unread support chats.
struct AttentionButton: View {
    let icon: String
    let count: Int
    let help: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            CIcon(icon, size: 16, color: Theme.onAttention)
                .frame(width: 28, height: 28)
                .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Theme.attention))
                .overlay(CountBadge(count: count)
                            .alignmentGuide(.trailing) { $0[HorizontalAlignment.center] }
                            .alignmentGuide(.top) { $0[VerticalAlignment.center] },
                         alignment: .topTrailing)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

/// Round accent count (a pill past 9), no border.
struct CountBadge: View {
    let count: Int

    var body: some View {
        Text(count > 99 ? "99+" : "\(count)")
            .font(.system(size: 9, weight: .bold))
            .foregroundColor(Theme.accentOn)
            .padding(.horizontal, count < 10 ? 0 : 4)
            .frame(minWidth: 15, minHeight: 15)
            .background(Capsule().fill(Theme.accent))
            .fixedSize()
    }
}

/// Count in the yellow attention style, for list rows (support chats).
struct AttentionCount: View {
    let count: Int

    var body: some View {
        Text(count > 99 ? "99+" : "\(count)")
            .font(.system(size: 11, weight: .bold))
            .foregroundColor(Theme.onAttention)
            .padding(.horizontal, count < 10 ? 0 : 6)
            .frame(minWidth: 20, minHeight: 20)
            .background(Capsule().fill(Theme.attention))
            .fixedSize()
    }
}

/// Pending one-time links beside "+".
struct PendingLinksButton: View {
    let count: Int
    let action: () -> Void

    var body: some View {
        AttentionButton(icon: "link-minimalistic-2", count: count,
                        help: "\(count) pending one-time link\(count == 1 ? "" : "s")", action: action)
    }
}

/// Unused one-time links: copy (short / full), name, delete (asks first).
/// Anyone holding one can still connect, so they stay visible here.
struct PendingLinksSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var renaming: Int64?
    @State private var draft = ""
    @State private var deleting: Invitation?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                CIcon("link-minimalistic-2", size: 18, color: Theme.attention)
                    .frame(width: 36, height: 36)
                    .background(Circle().fill(Theme.accentSoft))
                Text("Pending links").font(.cBodyBold).foregroundColor(Theme.primaryText)
                Spacer(minLength: 0)
            }
            Text("One-time links you created that nobody has used yet. Anyone who has one can still connect \u{2014} delete the ones you no longer need.")
                .font(.cMeta)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            if model.pendingInvitations.isEmpty {
                Text("No pending links.").font(.cMeta).foregroundColor(Theme.mutedText)
            }
            ScrollView {
                VStack(spacing: 10) {
                    ForEach(model.pendingInvitations) { inv in
                        row(inv)
                    }
                }
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(SecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
        .frame(width: 460, height: 480)
        .background(Theme.panel)
        .onAppear { model.refreshPendingInvitations() }
        .alert("Delete this link?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Cancel", role: .cancel) { deleting = nil }
            Button("Delete", role: .destructive) {
                if let inv = deleting {
                    Task { await model.deleteInvitation(connId: inv.connId) }
                }
                deleting = nil
            }
        } message: {
            Text("It will stop working. Anyone you sent it to won't be able to connect with it.")
        }
    }

    private func row(_ inv: Invitation) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if renaming == inv.connId {
                HStack(spacing: 8) {
                    CursusField(placeholder: "Name, e.g. who it's for", text: $draft, onSubmit: { save(inv) })
                    Button("Save") { save(inv) }.buttonStyle(PrimaryButtonStyle(small: true))
                }
            } else {
                HStack(spacing: 8) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(inv.alias.isEmpty ? "Unnamed link" : inv.alias)
                            .font(.cBody)
                            .foregroundColor(inv.alias.isEmpty ? Theme.secondaryText : Theme.primaryText)
                        Text(subtitle(inv)).font(.cMeta).foregroundColor(Theme.mutedText)
                    }
                    Spacer(minLength: 0)
                    IconTool(icon: "trash-delete", help: "Delete link", size: 16) { deleting = inv }
                }
            }
            HStack(spacing: 6) {
                if inv.link.short != nil {
                    small("Copy", icon: "copy") { Clipboard.copy(inv.link.display) }
                    small("Copy full", icon: "copy") { Clipboard.copy(inv.link.full) }
                } else {
                    small("Copy", icon: "copy") { Clipboard.copy(inv.link.full) }
                }
                small("Name", icon: "edit-3") {
                    draft = inv.alias
                    renaming = inv.connId
                }
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.field))
        .overlay(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).strokeBorder(Theme.border, lineWidth: 1))
    }

    private func small(_ label: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 6) {
                CIcon(icon, size: 13)
                Text(label)
            }
        }
        .buttonStyle(SecondaryButtonStyle(small: true))
    }

    private func subtitle(_ inv: Invitation) -> String {
        var parts: [String] = []
        if let date = Dates.parse(inv.createdAt) { parts.append("Created \(Dates.listStamp(date))") }
        if inv.incognito { parts.append("incognito") }
        return parts.joined(separator: " \u{b7} ")
    }

    private func save(_ inv: Invitation) {
        let name = draft.trimmingCharacters(in: .whitespaces)
        renaming = nil
        Task { await model.setInvitationAlias(connId: inv.connId, alias: name) }
    }
}
