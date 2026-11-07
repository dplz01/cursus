import AppKit
import SwiftUI

struct NewChatSheet: View {
    enum Tab: String, CaseIterable, Identifiable {
        case invite = "Invite"
        case connect = "Connect"
        case address = "My address"
        case group = "New group"
        case channel = "New channel"
        var id: String { rawValue }
    }

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var tab: Tab = .invite

    var body: some View {
        VStack(spacing: 16) {
            Picker("", selection: $tab) {
                ForEach(Tab.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            Group {
                switch tab {
                case .invite: InviteTab()
                case .connect: ConnectTab(done: { dismiss() })
                case .address: AddressTab()
                case .group: GroupTab(done: { dismiss() })
                case .channel: ChannelTab()
                }
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
        .frame(width: 460, height: 540)
        .background(Theme.panel)
    }
}

/// Link text + QR + copy, shared by invite and address tabs.
struct LinkCard: View {
    let link: ConnLink

    var body: some View {
        VStack(spacing: 12) {
            if let qr = Images.qrCode(link.full) {
                Image(nsImage: qr)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 200, height: 200)
                    .padding(10)
                    .background(RoundedRectangle(cornerRadius: Radius.lg).fill(Color.white))
                    .overlay(RoundedRectangle(cornerRadius: Radius.lg).strokeBorder(Theme.border, lineWidth: 1))
            }
            LinkBox(text: link.display)
        }
    }
}

private struct InviteTab: View {
    @EnvironmentObject private var model: AppModel
    @State private var link: ConnLink?
    @State private var loading = false

    var body: some View {
        VStack(spacing: 14) {
            Text("Share a one-time link with someone you want to talk to. It works once; they can paste it in any SimpleX app.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            if let link = link {
                LinkCard(link: link)
            } else {
                Button(loading ? "Creating…" : "Create one-time link") {
                    loading = true
                    Task {
                        link = await model.createInvitation()
                        loading = false
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(loading)
            }
        }
    }
}

private struct ConnectTab: View {
    @EnvironmentObject private var model: AppModel
    let done: () -> Void
    @State private var link = ""
    @State private var working = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Paste a SimpleX link you received — a one-time invitation, an address, a group or a channel link.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 4) {
                CursusField(placeholder: "https://simplex.chat/… or simplex:/…", text: $link, onSubmit: connect)
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
    }

    private func connect() {
        let text = link.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !working else { return }
        working = true
        Task {
            let ok = await model.connect(link: text)
            working = false
            if ok { done() }
        }
    }
}

private struct AddressTab: View {
    @EnvironmentObject private var model: AppModel
    @State private var link: ConnLink?
    @State private var loading = false

    var body: some View {
        VStack(spacing: 14) {
            Text("Your address can be used many times. Anyone with it can send you a contact request; you accept or reject each one.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            if let link = link {
                LinkCard(link: link)
            } else {
                Button(loading ? "Loading…" : "Show my address") {
                    loading = true
                    Task {
                        link = await model.addressLink()
                        loading = false
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(loading)
            }
        }
    }
}

private struct GroupTab: View {
    @EnvironmentObject private var model: AppModel
    let done: () -> Void
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create a group, then invite contacts from it.")
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
            CursusField(placeholder: "Group name", text: $name)
            HStack {
                Spacer()
                Button("Create group") {
                    let trimmed = name.trimmingCharacters(in: .whitespaces)
                    Task {
                        await model.createGroup(name: trimmed)
                        done()
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
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
