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
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
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
            Text("Cursus").font(.largeTitle.weight(.semibold))
            Text(subtitle)
                .font(.callout)
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
            VStack(alignment: .leading, spacing: 10) {
                TextField("Display name", text: $name)
                SecureField("Database password", text: $password)
                SecureField("Repeat password", text: $confirm)
                Toggle("Remember password in Keychain", isOn: $remember)
                Text("The password encrypts your messages on this Mac. It can't be recovered if you forget it.")
                    .font(.caption)
                    .foregroundColor(Theme.mutedText)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .textFieldStyle(.roundedBorder)
            .frame(width: 320)
            Button {
                Task { await model.createAccount(name: name.trimmingCharacters(in: .whitespaces), password: password, remember: remember) }
            } label: {
                Text(model.busy ? "Creating…" : "Create profile").frame(width: 180)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .keyboardShortcut(.defaultAction)
            .disabled(problem != nil || model.busy)
            if let problem = problem, !password.isEmpty || !name.isEmpty {
                Text(problem).font(.caption).foregroundColor(Theme.secondaryText)
            }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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
            VStack(alignment: .leading, spacing: 10) {
                SecureField("Database password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .focused($focused)
                    .onSubmit(unlock)
                Toggle("Remember password in Keychain", isOn: $remember)
            }
            .frame(width: 300)
            Button(action: unlock) {
                Text(model.busy ? "Unlocking…" : "Unlock").frame(width: 160)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .keyboardShortcut(.defaultAction)
            .disabled(password.isEmpty || model.busy)
            Button("Forgot password?") { confirmWipe = true }
                .buttonStyle(.link)
                .font(.caption)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 40))
                .foregroundColor(.orange)
            Text("Cursus couldn't start the chat core").font(.title2.weight(.semibold))
            Text(reason)
                .font(.callout.monospaced())
                .textSelection(.enabled)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 520)
            Button("Try again") {
                model.phase = .starting
                Task { await model.bootstrap() }
            }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
