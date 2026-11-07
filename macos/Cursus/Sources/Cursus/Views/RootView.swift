import AppKit
import SwiftUI

struct RootView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        content
            .task { await model.bootstrap() }
            .alert("Cursus", isPresented: errorShown) {
                Button("OK", role: .cancel) { model.errorMessage = nil }
            } message: {
                Text(model.errorMessage ?? "")
            }
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .starting:
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).background(Theme.panel)
        case .welcome:
            WelcomeView()
        case .unlock:
            UnlockView()
        case .ready:
            MainView()
        case .failed(let reason):
            FailureView(reason: reason)
        }
    }

    private var errorShown: Binding<Bool> {
        Binding(get: { model.errorMessage != nil }, set: { if !$0 { model.errorMessage = nil } })
    }
}

/// App icon + name, shared by the welcome and unlock screens.
struct BrandHeader: View {
    let subtitle: String

    var body: some View {
        VStack(spacing: 10) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 88, height: 88)
            Text("Cursus").font(.system(size: 28, weight: .semibold)).foregroundColor(Theme.primaryText)
            Text(subtitle)
                .font(.cBody)
                .foregroundColor(Theme.secondaryText)
                .multilineTextAlignment(.center)
        }
    }
}

struct WelcomeView: View {
    @EnvironmentObject private var model: AppModel
    @State private var name = ""
    @State private var password = ""
    @State private var confirm = ""
    @State private var remember = true

    private var problem: String? {
        if name.trimmingCharacters(in: .whitespaces).isEmpty { return "Choose a display name." }
        if password.count < 8 { return "Use at least 8 characters for the password." }
        if password != confirm { return "The passwords don't match." }
        return nil
    }

    var body: some View {
        VStack(spacing: 22) {
            BrandHeader(subtitle: "Private messaging on SimpleX — no phone number, no user IDs.")
            Card(spacing: 10) {
                CursusField(placeholder: "Display name", text: $name)
                CursusField(placeholder: "Database password", text: $password, secure: true)
                CursusField(placeholder: "Repeat password", text: $confirm, secure: true)
                Toggle("Remember password in Keychain", isOn: $remember)
                    .toggleStyle(.checkbox)
                    .font(.cBody)
                Text("The password encrypts your messages on this Mac. It can't be recovered if you forget it.")
                    .font(.cMeta)
                    .foregroundColor(Theme.mutedText)
                    .fixedSize(horizontal: false, vertical: true)
                Button {
                    Task { await model.createAccount(name: name.trimmingCharacters(in: .whitespaces), password: password, remember: remember) }
                } label: {
                    Text(model.busy ? "Creating…" : "Create profile")
                }
                .buttonStyle(PrimaryButtonStyle(stretch: true))
                .keyboardShortcut(.defaultAction)
                .disabled(problem != nil || model.busy)
                .padding(.top, 4)
                if let problem = problem, !password.isEmpty || !name.isEmpty {
                    Text(problem).font(.cMeta).foregroundColor(Theme.secondaryText)
                }
            }
            .frame(width: 340)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.panel)
    }
}

struct UnlockView: View {
    @EnvironmentObject private var model: AppModel
    @State private var password = ""
    @State private var remember = Keychain.hasPassword
    @State private var confirmWipe = false
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 22) {
            BrandHeader(subtitle: model.info?.displayName.map { "Welcome back, \($0)." } ?? "Enter your database password.")
            Card(spacing: 10) {
                SecureField("Database password", text: $password)
                    .textFieldStyle(.plain)
                    .font(.cBody)
                    .focused($focused)
                    .onSubmit(unlock)
                    .padding(.vertical, 8)
                    .padding(.horizontal, 12)
                    .background(RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(Theme.fieldFill))
                    .overlay(RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                        .strokeBorder(focused ? Theme.accent : Theme.border, lineWidth: 1))
                Toggle("Remember password in Keychain", isOn: $remember)
                    .toggleStyle(.checkbox)
                    .font(.cBody)
                Button(action: unlock) {
                    Text(model.busy ? "Unlocking…" : "Unlock")
                }
                .buttonStyle(PrimaryButtonStyle(stretch: true))
                .keyboardShortcut(.defaultAction)
                .disabled(password.isEmpty || model.busy)
                .padding(.top, 4)
            }
            .frame(width: 320)
            Button("Forgot password?") { confirmWipe = true }
                .buttonStyle(.plain)
                .font(.cMeta)
                .foregroundColor(Theme.accent)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.panel)
        .onAppear { focused = true }
        .confirmationDialog("Erase Cursus data on this Mac?", isPresented: $confirmWipe) {
            Button("Erase everything", role: .destructive) {
                Task { await model.wipeAll() }
            }
        } message: {
            Text("Without the password the database can't be opened. Erasing deletes all chats and contacts stored on this Mac and starts over with a new profile.")
        }
    }

    private func unlock() {
        guard !password.isEmpty, !model.busy else { return }
        let pw = password
        Task {
            await model.unlock(password: pw, remember: remember)
            if model.phase == .ready { password = "" }
        }
    }
}

struct FailureView: View {
    let reason: String
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 16) {
            IconDisc(icon: "sf:exclamationmark.triangle", size: 56, tint: Theme.danger, fill: Theme.dangerSoft)
            Text("Cursus couldn't start the chat core").font(.cTitle).foregroundColor(Theme.primaryText)
            Text(reason)
                .font(.cMeta.monospaced())
                .foregroundColor(Theme.secondaryText)
                .textSelection(.enabled)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 520)
            Button("Try again") {
                model.phase = .starting
                Task { await model.bootstrap() }
            }
            .buttonStyle(PrimaryButtonStyle())
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.panel)
    }
}
