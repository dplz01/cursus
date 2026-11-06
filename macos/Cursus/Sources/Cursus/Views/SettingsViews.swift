import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct SettingsListView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        List(selection: $model.settingsPage) {
            ForEach(SettingsPage.allCases) { page in
                Label(page.title, systemImage: page.symbol).tag(page)
            }
        }
        .listStyle(.inset)
        .navigationTitle("Settings")
    }
}

struct SettingsDetailView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                switch model.settingsPage ?? .profile {
                case .profile: ProfilePage()
                case .privacy: PrivacyPage()
                case .notifications: NotificationsPage()
                case .about: AboutPage()
                }
            }
            .padding(28)
            .frame(maxWidth: 560, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Theme.panel)
        .navigationTitle((model.settingsPage ?? .profile).title)
    }
}

/// Titled group of settings rows.
struct SettingsSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.uppercased())
                .font(.caption.weight(.semibold))
                .foregroundColor(Theme.mutedText)
            VStack(alignment: .leading, spacing: 12) {
                content
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 10).fill(Theme.elevated))
        }
    }
}

private struct ProfilePage: View {
    @EnvironmentObject private var model: AppModel
    @State private var name = ""
    @State private var fullName = ""
    @State private var bio = ""
    /// nil = unchanged; .some(nil) = remove; .some(url) = new picture.
    @State private var image: String??
    @State private var saving = false

    private var shownImage: String? {
        if let pending = image { return pending }
        return model.user?.image
    }

    private var changed: Bool {
        guard let user = model.user else { return false }
        return name != user.displayName || fullName != user.fullName || bio != user.shortDescr || image != nil
    }

    var body: some View {
        SettingsSection(title: "Your profile") {
            HStack(spacing: 16) {
                Avatar(image: shownImage, name: name.isEmpty ? "?" : name, size: 72)
                VStack(alignment: .leading, spacing: 6) {
                    Button("Choose picture…", action: pickImage)
                    if shownImage != nil {
                        Button("Remove picture") { image = .some(nil) }
                            .buttonStyle(.link)
                    }
                }
            }
            TextField("Display name", text: $name).textFieldStyle(.roundedBorder)
            TextField("Full name (optional)", text: $fullName).textFieldStyle(.roundedBorder)
            TextField("Bio (optional)", text: $bio).textFieldStyle(.roundedBorder)
            Text("Your profile is sent only to your contacts — SimpleX servers never see it.")
                .font(.caption)
                .foregroundColor(Theme.mutedText)
            HStack {
                Spacer()
                Button(saving ? "Saving…" : "Save") {
                    saving = true
                    Task {
                        await model.updateProfile(
                            displayName: name.trimmingCharacters(in: .whitespaces),
                            fullName: fullName, bio: String(bio.prefix(160)), image: image)
                        image = nil
                        saving = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!changed || saving || name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .onAppear(perform: load)
        .onChange(of: model.user) { _ in load() }
    }

    private func load() {
        guard let user = model.user else { return }
        name = user.displayName
        fullName = user.fullName
        bio = user.shortDescr
    }

    private func pickImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url, let picked = NSImage(contentsOf: url) else { return }
        if let dataURL = Images.profileDataURL(from: picked) {
            image = .some(dataURL)
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
        SettingsSection(title: "Delivery receipts") {
            Toggle("Send receipts to contacts", isOn: Binding(
                get: { model.user?.sendRcptsContacts ?? true },
                set: { on in Task { await model.setDeliveryReceipts(groups: false, on: on) } }))
            Toggle("Send receipts in small groups", isOn: Binding(
                get: { model.user?.sendRcptsSmallGroups ?? true },
                set: { on in Task { await model.setDeliveryReceipts(groups: true, on: on) } }))
        }
        SettingsSection(title: "This Mac") {
            Toggle("Encrypt received files", isOn: Binding(
                get: { model.info?.encryptLocalFiles ?? true },
                set: { on in Task { await model.setEncryptLocalFiles(on) } }))
            Toggle("Remember database password in Keychain", isOn: $remember)
                .onChange(of: remember) { on in
                    if !on { Keychain.delete() }
                }
            if remember && !Keychain.hasPassword {
                Text("The password is saved the next time you unlock.")
                    .font(.caption)
                    .foregroundColor(Theme.mutedText)
            }
            HStack {
                Button("Lock now") { model.lock() }
                Spacer()
                Button("Erase all data…") { confirmWipe = true }
                    .foregroundColor(.red)
            }
        }
        .confirmationDialog("Erase all Cursus data on this Mac?", isPresented: $confirmWipe) {
            Button("Erase everything", role: .destructive) { Task { await model.wipeAll() } }
        } message: {
            Text("All chats, contacts and files stored on this Mac are deleted. This can't be undone.")
        }
    }
}

private struct NotificationsPage: View {
    @AppStorage(AppModel.notificationsEnabledKey) private var enabled = true
    @AppStorage(AppModel.notificationsHideTextKey) private var hideText = false

    var body: some View {
        SettingsSection(title: "Notifications") {
            Toggle("Show notifications for new messages", isOn: $enabled)
            Toggle("Hide message text in notifications", isOn: $hideText)
                .disabled(!enabled)
            if !Notifier.available {
                Text("Notifications need Cursus to run as Cursus.app.")
                    .font(.caption)
                    .foregroundColor(Theme.mutedText)
            }
        }
    }
}

private struct AboutPage: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        SettingsSection(title: "Cursus for Mac") {
            HStack(spacing: 14) {
                Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 56, height: 56)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Cursus \(model.info?.version ?? "")").font(.headline)
                    Text("A native client for the SimpleX network, built on the official SimpleX chat core.")
                        .font(.callout)
                        .foregroundColor(Theme.secondaryText)
                }
            }
            if let dir = model.info?.dataDir {
                HStack {
                    Text("Data folder").foregroundColor(Theme.secondaryText)
                    Spacer()
                    Button("Show in Finder") {
                        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: dir)
                    }
                }
            }
            Text("Licensed under AGPL-3.0-or-later. Third-party notices are in Cursus.app/Contents/Resources/licenses.")
                .font(.caption)
                .foregroundColor(Theme.mutedText)
        }
    }
}
