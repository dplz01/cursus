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
                case .address: AddressPage()
                case .privacy: PrivacyPage()
                case .notifications: NotificationsPage()
                case .appearance: AppearancePage()
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

/// Asks before a setting makes you less private (Linux: the same warnings).
enum PrivacyConfirm {
    static func ask(_ title: String, _ detail: String, action: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.alertStyle = .warning
        alert.addButton(withTitle: action)
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }
}

/// A toggle stored in UserDefaults; turning it to `riskyValue` asks first.
private struct GuardedToggle: View {
    let label: String
    var note: String? = nil
    let key: String
    let defaultValue: Bool
    let riskyValue: Bool
    let warning: (title: String, detail: String, action: String)
    @State private var value = false

    var body: some View {
        ToggleRow(label: label, note: note, isOn: Binding(
            get: { value },
            set: { new in
                if new == riskyValue && !PrivacyConfirm.ask(warning.title, warning.detail, action: warning.action) {
                    return
                }
                value = new
                UserDefaults.standard.set(new, forKey: key)
            }))
            .onAppear { value = Prefs.bool(key, default: defaultValue) }
    }
}

private struct PrivacyPage: View {
    @EnvironmentObject private var model: AppModel
    @AppStorage(Prefs.lockAfter) private var lockAfter = LockAfter.never.rawValue
    @State private var remember = Keychain.hasPassword
    @State private var confirmWipe = false

    var body: some View {
        SettingsSection(icon: "lock", title: "Lock", subtitle: "Who can open Cursus on this Mac.") {
            PrefRow(label: "Lock Cursus", note: "Closes your chats; the database password opens them again.") {
                Picker("", selection: $lockAfter) {
                    ForEach(LockAfter.allCases) { Text($0.label).tag($0.rawValue) }
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .controlSize(.small)
                .fixedSize()
            }
            ToggleRow(label: "Remember database password in Keychain",
                      note: remember && !Keychain.hasPassword ? "The password is saved the next time you unlock." : nil,
                      isOn: $remember)
                .onChange(of: remember) { on in
                    if !on { Keychain.delete() }
                }
            ActionTile(icon: "lock", title: "Lock now (⇧⌘L)") { model.lock() }
        }
        SettingsSection(icon: "shield-check", title: "Screen", subtitle: "What someone looking at your screen sees.") {
            GuardedToggle(label: "Show message previews", note: "The last message under each chat.",
                          key: Prefs.chatPreviews, defaultValue: true, riskyValue: true,
                          warning: ("Show message previews?",
                                    "The last message of every chat will be visible in the chat list to anyone who can see your screen.",
                                    "Show previews"))
            GuardedToggle(label: "Blur photos and videos", note: "Click to see them.",
                          key: Prefs.blurMedia, defaultValue: false, riskyValue: false,
                          warning: ("Stop blurring media?",
                                    "Photos and videos will be shown in full in your chats, visible to anyone who can see your screen.",
                                    "Show media"))
        }
        SettingsSection(icon: "download", title: "Files and links", subtitle: "What Cursus fetches and opens for you.") {
            GuardedToggle(label: "Download media automatically", note: "Images, voice messages and short videos.",
                          key: Prefs.autoDownload, defaultValue: true, riskyValue: true,
                          warning: ("Download media automatically?",
                                    "Images, voice messages and short videos from anyone who writes to you will download without asking — including from strangers in groups.",
                                    "Download automatically"))
            GuardedToggle(label: "Protect IP address",
                          note: "Ask before downloading from file servers you don't use.",
                          key: Prefs.protectIP, defaultValue: true, riskyValue: false,
                          warning: ("Stop protecting your IP address?",
                                    "Files will download from any server the sender chooses, including automatic downloads. Without Tor or VPN, those servers see your IP address.",
                                    "Turn off"))
            ToggleRow(label: "Encrypt stored files", isOn: Binding(
                get: { model.info?.encryptLocalFiles ?? true },
                set: { on in
                    if !on && !PrivacyConfirm.ask(
                        "Store new files unencrypted?",
                        "Photos, videos, voice messages and files you send or receive from now on will be saved readable on disk. Files already stored stay encrypted.",
                        action: "Store unencrypted") { return }
                    Task { await model.setEncryptLocalFiles(on) }
                }))
            GuardedToggle(label: "Remove tracking from links",
                          note: "Drops codes like utm_source before opening a link.",
                          key: Prefs.cleanLinks, defaultValue: true, riskyValue: false,
                          warning: ("Keep tracking codes in links?",
                                    "Links will open exactly as sent, including codes like utm_source or fbclid that let websites track where you came from.",
                                    "Keep codes"))
        }
        SettingsSection(icon: "check-read", title: "What contacts see",
                        subtitle: "Let senders see that their messages arrived.") {
            ToggleRow(label: "Delivery receipts to contacts", isOn: Binding(
                get: { model.user?.sendRcptsContacts ?? true },
                set: { on in
                    if on && !PrivacyConfirm.ask("Send delivery receipts?",
                                                 "Your contacts will see when their messages reach this Mac.",
                                                 action: "Send receipts") { return }
                    Task { await model.setDeliveryReceipts(groups: false, on: on) }
                }))
            ToggleRow(label: "Delivery receipts in small groups", isOn: Binding(
                get: { model.user?.sendRcptsSmallGroups ?? true },
                set: { on in
                    if on && !PrivacyConfirm.ask("Send delivery receipts?",
                                                 "Members of small groups will see when their messages reach this Mac.",
                                                 action: "Send receipts") { return }
                    Task { await model.setDeliveryReceipts(groups: true, on: on) }
                }))
        }
        SettingsSection(icon: "trash-delete", title: "Data on this Mac", subtitle: "Erase everything Cursus stored here.") {
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

/// Your SimpleX address: the link, sharing it on your profile, auto-accept.
private struct AddressPage: View {
    @EnvironmentObject private var model: AppModel
    @State private var link: ConnLink?
    @State private var autoAccept = false
    @State private var loading = true

    var body: some View {
        SettingsSection(icon: "link-minimalistic-2", title: "Your address",
                        subtitle: "A link anyone can use to send you a contact request. You can change who gets in.") {
            if let link = link {
                LinkBox(text: link.short ?? link.display, copyText: link.short ?? link.full)
            } else if loading {
                ProgressView().controlSize(.small).padding(8)
            }
            ToggleRow(label: "Share address on profile",
                      note: "Your contacts see it and can pass it on.",
                      isOn: Binding(
                        get: { !(model.user?.contactLink ?? "").isEmpty },
                        set: { on in Task { await model.setProfileAddress(on) } }))
            ToggleRow(label: "Auto-accept contact requests",
                      note: "Anyone with the link connects without asking you.",
                      isOn: Binding(
                        get: { autoAccept },
                        set: { on in Task { autoAccept = await model.setAddressAutoAccept(on) } }))
        }
        .task {
            link = await model.addressLink()
            autoAccept = await model.addressAutoAccept()
            loading = false
        }
    }
}

private struct AppearancePage: View {
    @AppStorage(Prefs.theme) private var theme = AppTheme.system.rawValue
    @AppStorage(Prefs.textSize) private var textSize = TextSize.medium.rawValue
    @AppStorage(Prefs.enterSends) private var enterSends = true

    var body: some View {
        SettingsSection(icon: "image-square", title: "Appearance", subtitle: "How Cursus looks on this Mac.") {
            PrefRow(label: "Theme") {
                Picker("", selection: $theme) {
                    ForEach(AppTheme.allCases) { Text($0.label).tag($0.rawValue) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()
            }
            PrefRow(label: "Text size") {
                Picker("", selection: $textSize) {
                    ForEach(TextSize.allCases) { Text($0.label).tag($0.rawValue) }
                }
                .pickerStyle(.menu)
                .labelsHidden()
                .controlSize(.small)
                .fixedSize()
            }
        }
        SettingsSection(icon: "edit-3", title: "Typing", subtitle: "The message field.") {
            ToggleRow(label: "Enter sends messages",
                      note: enterSends ? "Shift+Enter starts a new line." : "Enter starts a new line; ⌘Enter sends.",
                      isOn: $enterSends)
        }
    }
}

private struct NotificationsPage: View {
    @AppStorage(AppModel.notificationsEnabledKey) private var enabled = true

    var body: some View {
        SettingsSection(icon: "sf:bell", title: "Notifications",
                        subtitle: Notifier.available ? "Banners and sounds for new messages." : "Notifications need Cursus to run as Cursus.app.") {
            ToggleRow(label: "Show notifications for new messages", isOn: $enabled)
            GuardedToggle(label: "Hide message text in notifications",
                          note: "Banners say only “New message”.",
                          key: AppModel.notificationsHideTextKey, defaultValue: true, riskyValue: false,
                          warning: ("Show message text in notifications?",
                                    "Anyone who can see your screen will be able to read incoming messages, and Notification Center keeps them after Cursus closes.",
                                    "Show text"))
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
