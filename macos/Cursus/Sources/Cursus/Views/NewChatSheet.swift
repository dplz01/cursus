import AppKit
import SwiftUI

struct NewChatSheet: View {
    enum Tab: String, CaseIterable, Identifiable {
        case invite = "Invite"
        case connect = "Connect"
        case address = "My address"
        case group = "New group"
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
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
        .frame(width: 460, height: 520)
    }
}

/// Link text + QR + copy, shared by invite and address tabs.
struct LinkCard: View {
    let link: ConnLink
    @State private var copied = false

    var body: some View {
        VStack(spacing: 12) {
            if let qr = Images.qrCode(link.full) {
                Image(nsImage: qr)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 210, height: 210)
                    .padding(10)
                    .background(Color.white)
                    .cornerRadius(10)
            }
            Text(link.display)
                .font(.caption.monospaced())
                .textSelection(.enabled)
                .lineLimit(3)
                .truncationMode(.middle)
                .padding(8)
                .frame(maxWidth: .infinity)
                .background(RoundedRectangle(cornerRadius: 8).fill(Theme.accentSoft))
            Button(copied ? "Copied" : "Copy link") {
                Clipboard.copy(link.display)
                copied = true
            }
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
                .font(.callout)
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
                .buttonStyle(.borderedProminent)
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
                .font(.callout)
                .foregroundColor(Theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            TextField("https://simplex.chat/… or simplex:/…", text: $link)
                .textFieldStyle(.roundedBorder)
                .onSubmit(connect)
            HStack {
                Button("Paste") {
                    if let text = NSPasteboard.general.string(forType: .string) { link = text }
                }
                Spacer()
                Button(working ? "Connecting…" : "Connect", action: connect)
                    .buttonStyle(.borderedProminent)
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
                .font(.callout)
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
                .buttonStyle(.borderedProminent)
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
                .font(.callout)
                .foregroundColor(Theme.secondaryText)
            TextField("Group name", text: $name)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("Create group") {
                    let trimmed = name.trimmingCharacters(in: .whitespaces)
                    Task {
                        await model.createGroup(name: trimmed)
                        done()
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }
}
