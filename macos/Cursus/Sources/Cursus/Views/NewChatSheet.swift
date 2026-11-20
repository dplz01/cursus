import AppKit
import SwiftUI

struct NewChatSheet: View {
    enum Tab: String, CaseIterable, Identifiable {
        case connect = "Connect"
        case group = "New group"
        case channel = "New channel"
        var id: String { rawValue }
    }

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var tab: Tab = .connect

    var body: some View {
        VStack(spacing: 16) {
            Text("New conversation").font(.cTitle).foregroundColor(Theme.primaryText)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 8) {
                ForEach(Tab.allCases) { choice in
                    Button { NSApp.keyWindow?.makeFirstResponder(nil); tab = choice } label: {
                        VStack(spacing: 6) {
                            CIcon(choice == .connect ? "link-minimalistic-2" : choice == .group ? "user-group" : "channels",
                                  size: 22, color: Theme.accent)
                            Text(choice.rawValue).font(.cMeta)
                        }.frame(maxWidth: .infinity).padding(.vertical, 10)
                    }.buttonStyle(CreationActionStyle())
                        .overlay(RoundedRectangle(cornerRadius: Radius.control).strokeBorder(tab == choice ? Theme.accent : .clear, lineWidth: 1))
                        .accessibilityValue(tab == choice ? "Selected" : "")
                }
            }

            ZStack(alignment: .top) {
                ScrollView { ConnectTab(done: { dismiss() }) }
                    .opacity(tab == .connect ? 1 : 0).allowsHitTesting(tab == .connect)
                    .accessibilityHidden(tab != .connect)
                ScrollView { GroupTab(done: { dismiss() }) }
                    .opacity(tab == .group ? 1 : 0).allowsHitTesting(tab == .group)
                    .accessibilityHidden(tab != .group)
                ScrollView { ChannelTab() }
                    .opacity(tab == .channel ? 1 : 0).allowsHitTesting(tab == .channel)
                    .accessibilityHidden(tab != .channel)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(SecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
        .frame(width: 480, height: 560)
        .background(Theme.panel)
    }
}

/// Linux Connect card: a one-time link created as the sheet opens, its
/// incognito switch and "New link" (asks first: the shown link is deleted),
/// then joining with a pasted link. Closing the sheet deletes the link if it
/// was never copied — nobody can have it. Copied links stay listed under the
/// gold button beside "+". The reusable address is in Settings.
private struct ConnectTab: View {
    @EnvironmentObject private var model: AppModel
    let done: () -> Void
    @AppStorage(Prefs.incognito) private var incognito = false
    @State private var invitation: Invitation?
    @State private var copied = false
    @State private var busy = false
    @State private var link = ""
    @State private var working = false
    @State private var showInfo = false
    @State private var confirmNew = false
    @State private var active = true

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                IconDisc(icon: "link-minimalistic-2", size: 36)
                Text("Connect").font(.cBodyBold).foregroundColor(Theme.primaryText)
                Spacer(minLength: 0)
                IconTool(icon: "info-circle", help: "What do these do?", size: 18) { showInfo = true }
                    .popover(isPresented: $showInfo, arrowEdge: .bottom) { ConnectInfo() }
            }
            SectionLabel("Copy one-time link")
            Text("Share privately with one person. The link expires after they connect.")
                .font(.cMeta).foregroundColor(Theme.secondaryText)
            HStack(spacing: 6) {
                if let invitation = invitation {
                    LinkBox(text: invitation.link.display, onCopy: { copied = true })
                } else {
                    Text(busy ? "Creating link…" : "No link yet")
                        .font(.cMeta)
                        .foregroundColor(Theme.mutedText)
                        .padding(.vertical, 10)
                        .padding(.horizontal, 12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                IconTool(icon: incognito ? "incognito-filled" : "incognito",
                         help: incognito ? "Incognito on: they'll see a random profile" : "Incognito off: they'll see your profile",
                         active: incognito, size: 20, action: toggleIncognito).disabled(busy)
            }
            HStack {
                Spacer()
                // Secondary action keeps joining as the main action.
                Button(action: { if invitation == nil { create() } else { confirmNew = true } }) {
                    HStack(spacing: 6) {
                        CIcon("repeat-once", size: 14)
                        Text("New link")
                    }
                }
                .buttonStyle(SecondaryButtonStyle())
                .disabled(busy)
            }
            Divider().padding(.vertical, 6)
            SectionLabel("Join with a link")
            Text("Paste a contact, group or channel link to connect.")
                .font(.cMeta).foregroundColor(Theme.secondaryText)
            HStack(spacing: 4) {
                CursusField(placeholder: "Invite or address link…", text: $link, onSubmit: connect)
                IconTool(icon: "paste", help: "Paste") {
                    if let text = NSPasteboard.general.string(forType: .string) { link = text }
                }
            }
            HStack {
                Spacer()
                Button(working ? "Connecting…" : "Connect", action: connect)
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || working)
            }
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(Theme.elevated))
        .overlay(RoundedRectangle(cornerRadius: Radius.control).strokeBorder(Theme.border, lineWidth: 1))
        .onAppear { active = true; if invitation == nil { create() } }
        .onDisappear { active = false; dropUnshared() }
        .alert("Create a new link?", isPresented: $confirmNew) {
            Button("Cancel", role: .cancel) {}
            Button("Delete and create new", role: .destructive, action: regenerate)
        } message: {
            Text("The previous link will be deleted. Anyone you already sent it to won't be able to connect with it.")
        }
    }

    private func create() {
        guard !busy else { return }
        busy = true
        copied = false
        Task {
            let result = await model.newInvitation(incognito: incognito)
            if active { invitation = result }
            else if let result = result { await model.deleteInvitation(connId: result.connId, quiet: true) }
            busy = false
        }
    }

    private func regenerate() {
        guard !busy else { return }
        busy = true
        Task {
            if let inv = await model.regenerateInvitation(previous: invitation?.connId, incognito: incognito) {
                if active { invitation = inv }
                else { await model.deleteInvitation(connId: inv.connId, quiet: true) }
            }
            busy = false
        }
    }

    /// Never copied: nobody can have it, so don't leave it behind.
    private func dropUnshared() {
        guard let inv = invitation, !copied else { return }
        invitation = nil
        Task { await model.deleteInvitation(connId: inv.connId, quiet: true) }
    }

    private func toggleIncognito() {
        let on = !incognito
        incognito = on
        guard let current = invitation, !busy else { return }
        busy = true
        Task {
            if let state = await model.setInvitationIncognito(connId: current.connId, on: on) {
                invitation?.incognito = state
                incognito = state
            } else {
                invitation = nil // used or deleted meanwhile
                model.refreshPendingInvitations()
            }
            busy = false
        }
    }

    private func connect() {
        let text = link.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !working else { return }
        working = true
        Task {
            let ok = await model.connect(link: text, incognito: incognito)
            working = false
            if ok { done() }
        }
    }
}

/// What the Connect card's parts do (same text as the Linux ⓘ sheet).
private struct ConnectInfo: View {
    private let paragraphs = [
        "One-time link: a new one is made each time you open this page. Send it to one person \u{2014} whoever opens it first becomes your contact, then it stops working. \u{201c}New link\u{201d} deletes the shown link and makes a fresh one.",
        "Links you copied that nobody has used yet stay valid: the gold link button next to + lists them, to copy, name or delete. A link you never copied is deleted when you leave this page \u{2014} nobody can have it.",
        "Incognito: when on, the person who connects sees a new random name instead of your profile \u{2014} a different one for each contact. You can switch it until the link is used. It also applies when you connect with a link below.",
        "Join with a link: paste a link someone sent you \u{2014} a one-time link, an address, or a group or channel link \u{2014} and click Connect.",
        "Your reusable address, which many people can use, is in Settings \u{2192} Address.",
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                CIcon("info-circle", size: 18, color: Theme.accent)
                Text("Connect").font(.cBodyBold).foregroundColor(Theme.primaryText)
            }
            ForEach(paragraphs, id: \.self) { text in
                Text(text)
                    .font(.cMeta)
                    .foregroundColor(Theme.secondaryText)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(16)
        .frame(width: 340)
    }
}

private struct GroupTab: View {
    @EnvironmentObject private var model: AppModel
    let done: () -> Void
    @State private var name = ""
    @State private var creating = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create a group, then invite contacts from it.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
            CursusField(placeholder: "Group name", text: $name)
            HStack {
                Spacer()
                Button("Create group") {
                    guard !creating else { return }
                    let trimmed = name.trimmingCharacters(in: .whitespaces)
                    creating = true
                    Task {
                        let success = await model.createGroup(name: trimmed)
                        creating = false
                        if success { done() }
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(creating || name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }
}

/// A channel: relay-backed, anyone with the link subscribes; only owners post.
private struct ChannelTab: View {
    @EnvironmentObject private var model: AppModel
    @State private var name = ""
    @State private var creating = false
    @State private var link: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Channels broadcast to subscribers through chat relays. Share the link so people can subscribe.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            if let link = link {
                Text("Channel created. Share this link:")
                    .font(.cBodyBold)
                    .foregroundColor(Theme.primaryText)
                LinkBox(text: link)
            } else {
                CursusField(placeholder: "Channel name", text: $name)
                HStack {
                    Spacer()
                    Button(creating ? "Creating…" : "Create channel") {
                        let trimmed = name.trimmingCharacters(in: .whitespaces)
                        creating = true
                        Task {
                            link = await model.createChannel(name: trimmed)
                            creating = false
                        }
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(creating || name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }
}
