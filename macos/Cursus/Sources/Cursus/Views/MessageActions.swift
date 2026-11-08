import AppKit
import SwiftUI

// Message actions beyond reply / edit / delete: forward, info, selecting
// several messages (Linux: the message menu and the selection bar).

// MARK: - Forward

/// Pick the chat to forward to (native sheet, Cursus card inside).
struct ForwardSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let ids: [Int64]
    @State private var search = ""

    private var targets: [Chat] {
        let all = model.chats.filter { $0.pendingRequestId == nil }
        let q = search.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return all }
        return all.filter { $0.title.localizedCaseInsensitiveContains(q) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(ids.count == 1 ? "Forward message" : "Forward \(ids.count) messages")
                        .font(.cBodyBold).foregroundColor(Theme.primaryText)
                    Text("Choose a chat.").font(.cMeta).foregroundColor(Theme.secondaryText)
                }
                Spacer()
                IconTool(icon: "cancel", help: "Cancel (Esc)", size: 16) { close() }
                    .keyboardShortcut(.cancelAction)
            }
            SearchField(text: $search)
            ScrollView {
                LazyVStack(spacing: 2) {
                    ForEach(targets) { chat in
                        Button {
                            Task { await model.forward(ids, to: chat) }
                            dismiss()
                        } label: {
                            HStack(spacing: 10) {
                                Avatar(image: chat.image, name: chat.title, size: 32, kind: chat.avatarKind)
                                Text(chat.title).font(.cBody).foregroundColor(Theme.primaryText).lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .padding(.vertical, 6)
                            .padding(.horizontal, 8)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(GhostTileStyle())
                    }
                }
            }
            .frame(height: 320)
        }
        .padding(20)
        .frame(width: 420)
        .background(Theme.elevated)
    }

    private func close() {
        model.forwarding = nil
        dismiss()
    }
}

// MARK: - Info

/// Time, delivery status and text of one message.
struct MessageInfoSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let message: Message
    @State private var info: MessageInfo?
    @State private var loading = true

    private static let full: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .medium
        return f
    }()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Message info").font(.cBodyBold).foregroundColor(Theme.primaryText)
                Spacer()
                IconTool(icon: "cancel", help: "Close (Esc)", size: 16) { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            if loading {
                ProgressView().controlSize(.small)
            } else if let info = info {
                if let date = info.date ?? message.date {
                    row(message.outgoing ? "Sent" : "Received", Self.full.string(from: date))
                }
                if let status = info.statusLabel {
                    row("Status", status)
                }
                if message.edited { row("Edited", "Yes") }
                if let file = message.file {
                    row("File", "\(file.name) · \(ByteCountFormatter.string(fromByteCount: file.size, countStyle: .file))")
                }
                if !info.text.isEmpty {
                    SectionLabel("Text")
                    Text(info.text)
                        .font(.cBody)
                        .foregroundColor(Theme.primaryText)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 8)
                }
            }
        }
        .padding(20)
        .frame(width: 400, alignment: .leading)
        .background(Theme.elevated)
        .task {
            info = await model.messageInfo(message)
            loading = false
        }
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label).font(.cMeta).foregroundColor(Theme.secondaryText).frame(width: 80, alignment: .leading)
            Text(value).font(.cBody).foregroundColor(Theme.primaryText).textSelection(.enabled)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 8)
    }
}

// MARK: - Selecting several messages

/// Replaces the composer while selecting: count, ✕, Forward, Delete.
struct SelectionBar: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        let n = model.selectedMessages.count
        HStack(spacing: 8) {
            IconTool(icon: "cancel", help: "Cancel (Esc)", size: 16) { model.stopSelecting() }
                .keyboardShortcut(.cancelAction)
            Text(n == 1 ? "1 selected" : "\(n) selected").font(.cBody).foregroundColor(Theme.primaryText)
            Spacer()
            Button("Forward") { model.forwardSelected() }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(n == 0)
            Button("Delete for me") { Task { await model.deleteSelected() } }
                .buttonStyle(DangerButtonStyle())
                .disabled(n == 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Theme.list)
    }
}

/// Round check in front of a message while selecting.
struct SelectionMark: View {
    let selected: Bool

    var body: some View {
        ZStack {
            Circle().strokeBorder(selected ? Theme.accent : Theme.mutedText, lineWidth: 1.5)
            if selected {
                Circle().fill(Theme.accent)
                Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundColor(Theme.accentOn)
            }
        }
        .frame(width: 20, height: 20)
    }
}

// MARK: - Who reacted

/// Reaction chip: toggles your reaction; in groups, its menu lists who reacted.
struct ReactionChip: View {
    @EnvironmentObject private var model: AppModel
    let message: Message
    let reaction: Reaction
    let inGroup: Bool
    @State private var showMembers = false
    @State private var members: [String]?

    var body: some View {
        Button {
            Task { await model.toggleReaction(message, emoji: reaction.emoji) }
        } label: {
            Text("\(reaction.glyph) \(reaction.total)")
                .font(.cMeta)
                .foregroundColor(reaction.mine ? Theme.accent : Theme.primaryText)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(RoundedRectangle(cornerRadius: 14).fill(reaction.mine ? Theme.accentSoft : Theme.elevated))
                .overlay(RoundedRectangle(cornerRadius: 14)
                    .strokeBorder(reaction.mine ? Theme.accent : Theme.border, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .help(reaction.mine ? "Remove your reaction" : "React \(reaction.glyph)")
        .contextMenu {
            if inGroup {
                Button("Who reacted…") { showMembers = true }
            }
        }
        .popover(isPresented: $showMembers, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 6) {
                Text("\(reaction.glyph) \(reaction.total)").font(.cBodyBold)
                if let members = members {
                    ForEach(members, id: \.self) { name in
                        Text(name).font(.cBody).foregroundColor(Theme.primaryText)
                    }
                } else {
                    ProgressView().controlSize(.small)
                }
            }
            .padding(12)
            .frame(minWidth: 160, alignment: .leading)
            .task { members = await model.reactionMembers(message, emoji: reaction.emoji) }
        }
    }
}
