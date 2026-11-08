import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Profile summary on top, then the categories as rows with icon discs;
/// the selected one is accent.soft and opens as cards in the main pane.
struct SettingsListView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            PaneHeader(title: "Settings", background: Theme.list)
            list
        }
        .background(Theme.list)
    }

    private var list: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                if let user = model.user {
                    Button { model.settingsPage = .profile } label: {
                        HStack(spacing: 12) {
                            Avatar(image: user.image, name: user.displayName, size: 52)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(user.displayName).font(.cTitle).foregroundColor(Theme.primaryText).lineLimit(1)
                                Text(user.shortDescr.isEmpty ? "Your profile" : user.shortDescr)
                                    .font(.cMeta)
                                    .foregroundColor(Theme.secondaryText)
                                    .lineLimit(2)
                            }
                            Spacer(minLength: 0)
                        }
                        .padding(10)
                    }
                    .buttonStyle(GhostTileStyle())
                    .padding(.bottom, 6)
                }
                ForEach(SettingsPage.allCases) { page in
                    NavRow(icon: page.icon, title: page.title, subtitle: page.subtitle,
                           selected: model.settingsPage == page) {
                        model.settingsPage = page
                    }
                }
            }
            .padding(8)
        }
        .background(Theme.list)
    }
}

struct SettingsDetailView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            PaneHeader(title: (model.settingsPage ?? .profile).title)
            Page {
                switch model.settingsPage ?? .profile {
                case .profile: ProfilePage()
                case .privacy: PrivacyPage()
                case .notifications: NotificationsPage()
                case .about: AboutPage()
                }
            }
        }
        .background(Theme.panel)
    }
}

/// A settings card: title row, then its rows.
struct SettingsSection<Content: View>: View {
    let icon: String
    let title: String
    var subtitle: String? = nil
    @ViewBuilder let content: Content

    var body: some View {
        Card(spacing: 6) {
            CardTitle(icon: icon, title: title, subtitle: subtitle)
                .padding(.bottom, 4)
            content
        }
        .toggleStyle(.switch)
        .tint(Theme.accent)
    }
}

/// Toggle on the right of a preference row.
struct ToggleRow: View {
    let label: String
    var note: String? = nil
    @Binding var isOn: Bool

    var body: some View {
        PrefRow(label: label, note: note) {
            Toggle("", isOn: $isOn).labelsHidden().controlSize(.small)
        }
    }
}

/// Your profile, like the Linux page: picture actions under the avatar,
/// then name, bio and description as text with a pencil; each edit is saved
/// on its own (no Save button).
private struct ProfilePage: View {
    @EnvironmentObject private var model: AppModel
    @State private var editing: OwnField?

    private enum OwnField: String, Identifiable {
        case name, bio, description
        var id: String { rawValue }
    }

    var body: some View {
        if let user = model.user {
            SettingsSection(icon: "user-alt-1", title: "Your profile",
                            subtitle: "Sent only to your contacts — SimpleX servers never see it.") {
                HStack(alignment: .center, spacing: 14) {
                    VStack(spacing: 8) {
                        Avatar(image: user.image, name: user.displayName, size: 72)
                        HStack(spacing: 6) {
                            IconTool(icon: "image-pen", help: "Choose picture…", size: 18, action: pickImage)
                            if user.image != nil {
                                IconTool(icon: "cancel", help: "Remove picture", size: 18) {
                                    Task { await model.updateProfile(image: .some(nil)) }
                                }
                            }
                        }
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        EditableLine(text: user.displayName, placeholder: "Display name", style: .name) {
                            editing = .name
                        }
                        EditableLine(text: user.shortDescr, placeholder: "Add a bio", style: .sub) {
                            editing = .bio
                        }
                    }
                }
                SectionLabel("Description")
                EditableLine(text: user.description, placeholder: "Add a longer description", style: .body) {
                    editing = .description
                }
            }
            .sheet(item: $editing) { field in editor(field, user) }
        }
    }

    private func editor(_ field: OwnField, _ user: User) -> TextEditSheet {
        switch field {
        case .name:
            return TextEditSheet(title: "Display name", hint: "How your contacts see you.", placeholder: "Your name",
                                 required: true, initial: user.displayName) { v in
                Task { await model.updateProfile(displayName: v) }
            }
        case .bio:
            return TextEditSheet(title: "Bio", hint: "A short line under your name.", placeholder: "Bio",
                                 initial: user.shortDescr) { v in
                Task { await model.updateProfile(bio: String(v.prefix(160))) }
            }
        case .description:
            return TextEditSheet(title: "Description", hint: "Shown on your profile.",
                                 placeholder: "Tell people about yourself", multiline: true,
                                 initial: user.description) { v in
                Task { await model.updateProfile(description: v) }
            }
        }
    }

    private func pickImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url, let picked = NSImage(contentsOf: url) else { return }
        if let dataURL = Images.profileDataURL(from: picked) {
            Task { await model.updateProfile(image: .some(dataURL)) }
        } else {
            model.errorMessage = "That picture couldn't be used."
        }
    }
}

private struct PrivacyPage: View {
    @EnvironmentObject private var model: AppModel
    @State private var remember = Keychain.hasPassword
    @State private var confirmWipe = false

    var body: some View {
        SettingsSection(icon: "check-read", title: "Delivery receipts",
                        subtitle: "Let senders see that their messages arrived.") {
            ToggleRow(label: "Send receipts to contacts", isOn: Binding(
                get: { model.user?.sendRcptsContacts ?? true },
                set: { on in Task { await model.setDeliveryReceipts(groups: false, on: on) } }))
            ToggleRow(label: "Send receipts in small groups", isOn: Binding(
                get: { model.user?.sendRcptsSmallGroups ?? true },
                set: { on in Task { await model.setDeliveryReceipts(groups: true, on: on) } }))
        }
        SettingsSection(icon: "lock", title: "This Mac", subtitle: "How Cursus keeps your data here.") {
            ToggleRow(label: "Encrypt received files", isOn: Binding(
                get: { model.info?.encryptLocalFiles ?? true },
                set: { on in Task { await model.setEncryptLocalFiles(on) } }))
            ToggleRow(label: "Remember database password in Keychain",
                      note: remember && !Keychain.hasPassword ? "The password is saved the next time you unlock." : nil,
                      isOn: $remember)
                .onChange(of: remember) { on in
                    if !on { Keychain.delete() }
                }
            ActionTile(icon: "lock", title: "Lock now") { model.lock() }
            if confirmWipe {
                InlineConfirm(question: "Erase all Cursus data on this Mac?",
                              detail: "All chats, contacts and files stored on this Mac are deleted. This can't be undone.",
                              confirmTitle: "Erase everything",
                              onCancel: { confirmWipe = false },
                              onConfirm: {
                                  confirmWipe = false
                                  Task { await model.wipeAll() }
                              })
            } else {
                ActionTile(icon: "trash-delete", title: "Erase all data…", danger: true) { confirmWipe = true }
            }
        }
    }
}

private struct NotificationsPage: View {
    @AppStorage(AppModel.notificationsEnabledKey) private var enabled = true
    @AppStorage(AppModel.notificationsHideTextKey) private var hideText = false

    var body: some View {
        SettingsSection(icon: "sf:bell", title: "Notifications",
                        subtitle: Notifier.available ? "Banners and sounds for new messages." : "Notifications need Cursus to run as Cursus.app.") {
            ToggleRow(label: "Show notifications for new messages", isOn: $enabled)
            ToggleRow(label: "Hide message text in notifications", isOn: $hideText)
                .disabled(!enabled)
        }
    }
}

private struct AboutPage: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Card {
            HStack(spacing: 14) {
                Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 56, height: 56)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Cursus \(model.info?.version ?? "")").font(.cTitle).foregroundColor(Theme.primaryText)
                    Text("A native client for the SimpleX network, built on the official SimpleX chat core.")
                        .font(.cMeta)
                        .foregroundColor(Theme.secondaryText)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        if let dir = model.info?.dataDir {
            Card(spacing: 4) {
                NavRow(icon: "sf:folder", title: "Data folder", subtitle: "Show in Finder") {
                    NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: dir)
                }
                NavRow(icon: "review-file", title: "Licences", subtitle: "AGPL-3.0-or-later and third-party notices") {
                    let dir = Bundle.main.bundlePath + "/Contents/Resources/licenses"
                    NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: dir)
                }
            }
        }
    }
}
