import AppKit
import Foundation
import SwiftUI

enum Phase: Equatable {
    case starting
    /// First run: create profile + database password.
    case welcome
    case unlock
    case ready
    /// libsimplex could not start (wrong platform, missing files, …).
    case failed(String)
}

enum Destination: String, CaseIterable, Identifiable {
    case chats, channels, calls, settings
    var id: String { rawValue }

    var title: String {
        switch self {
        case .chats: return "Chats"
        case .channels: return "Channels"
        case .calls: return "Calls"
        case .settings: return "Settings"
        }
    }

    var symbol: String {
        switch self {
        case .chats: return "bubble.left.and.bubble.right"
        case .channels: return "megaphone"
        case .calls: return "phone"
        case .settings: return "gearshape"
        }
    }
}

enum SettingsPage: String, CaseIterable, Identifiable {
    case profile, privacy, notifications, about
    var id: String { rawValue }

    var title: String {
        switch self {
        case .profile: return "Profile"
        case .privacy: return "Privacy & security"
        case .notifications: return "Notifications"
        case .about: return "About"
        }
    }

    var symbol: String {
        switch self {
        case .profile: return "person.crop.circle"
        case .privacy: return "lock.shield"
        case .notifications: return "bell"
        case .about: return "info.circle"
        }
    }
}

/// What the composer is doing besides writing a new message.
enum ComposeMode: Equatable {
    case new
    case reply(Message)
    case edit(Message)
}

@MainActor
final class AppModel: ObservableObject {
    @Published var phase: Phase = .starting
    @Published var info: AppInfo?
    @Published var user: User?
    @Published var chats: [Chat] = []
    @Published var destination: Destination = .chats
    @Published var settingsPage: SettingsPage? = .profile
    @Published var selection: String? {
        didSet {
            if selection != oldValue { selectionChanged(from: oldValue) }
        }
    }
    @Published var messages: [Message] = []
    /// Live XFTP progress by file id (between message reloads).
    @Published var fileProgress: [Int64: Double] = [:]
    @Published var composeMode: ComposeMode = .new
    @Published var draft = ""
    @Published var errorMessage: String?
    @Published var busy = false
    @Published var showNewChat = false
    @Published var isLoadingMessages = false

    /// Settings → Notifications (`@AppStorage` with these keys in the view).
    static let notificationsEnabledKey = "notificationsEnabled"
    static let notificationsHideTextKey = "notificationsHideText"
    private var notificationsEnabled: Bool {
        UserDefaults.standard.object(forKey: Self.notificationsEnabledKey) as? Bool ?? true
    }
    private var notificationsHideText: Bool {
        UserDefaults.standard.bool(forKey: Self.notificationsHideTextKey)
    }

    private var pump: EventPump?
    private var chatsRefresh: Task<Void, Never>?
    private var messagesRefresh: Task<Void, Never>?
    private var drafts: [String: String] = [:]
    private let pageSize = 120

    /// libsimplex inside the app bundle (Contents/Frameworks); nil when run
    /// from the repo (`swift run`), where the core finds third_party/.
    let libsDir: String? = {
        guard let dir = Bundle.main.privateFrameworksPath else { return nil }
        return FileManager.default.fileExists(atPath: dir + "/libsimplex.dylib") ? dir : nil
    }()

    var selectedChat: Chat? {
        guard let selection = selection else { return nil }
        return chats.first { $0.ref == selection }
    }

    var visibleChats: [Chat] {
        let wantChannels = destination == .channels
        return chats.filter { $0.isChannel == wantChannels }
    }

    var unreadTotal: Int {
        chats.filter { !$0.muted }.reduce(0) { $0 + max($1.unread, $1.unreadChat ? 1 : 0) }
    }

    // MARK: - Startup / unlock

    func bootstrap() async {
        if pump == nil {
            let pump = EventPump { [weak self] event in
                self?.handle(event)
            }
            pump.start()
            self.pump = pump
        }
        do {
            let info = try await Core.call("appInfo", as: AppInfo.self)
            self.info = info
            if !info.setupComplete && !info.databaseExists {
                phase = .welcome
                return
            }
            phase = .unlock
            if let saved = Keychain.load() {
                await unlock(password: saved, remember: true, quiet: true)
            }
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func createAccount(name: String, password: String, remember: Bool) async {
        await open(params: [
            "password": password,
            "displayName": name,
            "encryptLocalFiles": true,
            "saveAccount": true,
        ], password: password, remember: remember, quiet: false)
    }

    func unlock(password: String, remember: Bool, quiet: Bool = false) async {
        let name = info?.displayName ?? ""
        await open(params: [
            "password": password,
            "displayName": name,
            "encryptLocalFiles": info?.encryptLocalFiles ?? true,
            "saveAccount": !(info?.setupComplete ?? false),
        ], password: password, remember: remember, quiet: quiet)
    }

    private func open(params: [String: Any], password: String, remember: Bool, quiet: Bool) async {
        var params = params
        if let libsDir = libsDir { params["libsDir"] = libsDir }
        busy = true
        defer { busy = false }
        do {
            user = try await Core.call("open", params, as: User.self)
            if remember {
                Keychain.save(password)
            }
            info = try? await Core.call("appInfo", as: AppInfo.self)
            phase = .ready
            Notifier.requestAuthorization()
            await reloadChats()
        } catch {
            let text = error.localizedDescription
            if quiet {
                // A stale Keychain entry: forget it and ask.
                Keychain.delete()
            } else if text.lowercased().contains("wrong") || text.lowercased().contains("passphrase")
                        || text.lowercased().contains("key") {
                errorMessage = "Wrong password.\n\n\(text)"
            } else {
                errorMessage = text
            }
        }
    }

    func lock() {
        Task {
            try? await Core.run("close")
        }
        selection = nil
        chats = []
        messages = []
        user = nil
        drafts = [:]
        draft = ""
        composeMode = .new
        NSApp.dockTile.badgeLabel = nil
        phase = .unlock
    }

    /// Forget everything on this Mac (forgotten password).
    func wipeAll() async {
        do {
            try await Core.run("wipeLocalData")
            Keychain.delete()
            selection = nil
            chats = []
            messages = []
            user = nil
            info = try? await Core.call("appInfo", as: AppInfo.self)
            phase = .welcome
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: - Chats

    func reloadChats() async {
        guard phase == .ready else { return }
        do {
            let list = try await Core.call("listChats", as: [Chat].self)
            if list != chats { chats = list }
            if let sel = selection, !list.contains(where: { $0.ref == sel }) {
                selection = nil
            }
            NSApp.dockTile.badgeLabel = unreadTotal > 0 ? "\(unreadTotal)" : nil
        } catch {
            report(error)
        }
    }

    /// Coalesce bursts of events into one list reload.
    func scheduleChatsReload() {
        chatsRefresh?.cancel()
        chatsRefresh = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 150_000_000)
            guard !Task.isCancelled else { return }
            await self?.reloadChats()
        }
    }

    private func selectionChanged(from old: String?) {
        if let old = old {
            drafts[old] = composeMode == .new ? draft : nil
        }
        draft = selection.flatMap { drafts[$0] } ?? ""
        composeMode = .new
        messages = []
        fileProgress = [:]
        guard let ref = selection else { return }
        isLoadingMessages = true
        Task {
            await reloadMessages(for: ref)
            isLoadingMessages = false
            await markRead(ref)
        }
    }

    func reloadMessages(for ref: String) async {
        do {
            let list = try await Core.call("listMessages", ["chat": ref, "count": pageSize], as: [Message].self)
            guard selection == ref else { return }
            if list != messages { messages = list }
        } catch {
            report(error)
        }
    }

    func scheduleMessagesReload() {
        guard let ref = selection else { return }
        messagesRefresh?.cancel()
        messagesRefresh = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 120_000_000)
            guard !Task.isCancelled else { return }
            await self?.reloadMessages(for: ref)
        }
    }

    func markRead(_ ref: String) async {
        guard let chat = chats.first(where: { $0.ref == ref }), chat.hasUnread else { return }
        try? await Core.run("markRead", ["chat": ref])
        Notifier.clear(chat: ref)
        await reloadChats()
    }

    func markUnread(_ chat: Chat) async {
        await perform { try await Core.run("markUnread", ["chat": chat.ref]) }
    }

    func setFavorite(_ chat: Chat, _ on: Bool) async {
        await perform {
            try await Core.run("setChatSettings", [
                "chat": chat.ref, "favorite": on, "notifications": chat.notifications,
            ])
        }
    }

    func setMuted(_ chat: Chat, _ muted: Bool) async {
        await perform {
            try await Core.run("setChatSettings", [
                "chat": chat.ref, "favorite": chat.favorite, "notifications": muted ? "none" : "all",
            ])
        }
    }

    func clear(_ chat: Chat) async {
        await perform { try await Core.run("clearChat", ["chat": chat.ref]) }
        if selection == chat.ref { await reloadMessages(for: chat.ref) }
    }

    func delete(_ chat: Chat) async {
        await perform { try await Core.run("deleteChat", ["chat": chat.ref]) }
        if selection == chat.ref { selection = nil }
    }

    func acceptRequest(_ chat: Chat) async {
        guard let id = chat.pendingRequestId else { return }
        await perform { try await Core.run("acceptRequest", ["id": id]) }
    }

    func rejectRequest(_ chat: Chat) async {
        guard let id = chat.pendingRequestId else { return }
        await perform { try await Core.run("rejectRequest", ["id": id]) }
        if selection == chat.ref { selection = nil }
    }

    func joinGroup(_ groupId: Int64) async {
        await perform { try await Core.run("joinGroup", ["groupId": groupId]) }
    }

    // MARK: - Messages

    func send() async {
        guard let ref = selection else { return }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        let mode = composeMode
        draft = ""
        composeMode = .new
        do {
            switch mode {
            case .edit(let message):
                try await Core.run("updateText", ["chat": ref, "itemId": message.id, "text": text])
            case .reply(let message):
                try await Core.run("sendText", ["chat": ref, "text": text, "quote": message.id])
            case .new:
                try await Core.run("sendText", ["chat": ref, "text": text])
            }
            await reloadMessages(for: ref)
            scheduleChatsReload()
        } catch {
            draft = text
            composeMode = mode
            report(error)
        }
    }

    func startEdit(_ message: Message) {
        composeMode = .edit(message)
        draft = message.text
    }

    func startReply(_ message: Message) {
        composeMode = .reply(message)
    }

    func cancelCompose() {
        if case .edit = composeMode { draft = "" }
        composeMode = .new
    }

    func sendFiles(_ urls: [URL]) async {
        guard let ref = selection else { return }
        for url in urls {
            do {
                try await Core.run("sendFile", ["chat": ref, "path": url.path])
            } catch {
                report(error)
            }
        }
        await reloadMessages(for: ref)
        scheduleChatsReload()
    }

    func toggleReaction(_ message: Message, emoji: String) async {
        guard let ref = selection else { return }
        let mine = message.reactions.contains { $0.emoji == emoji && $0.mine }
        await perform {
            try await Core.run("react", ["chat": ref, "itemId": message.id, "emoji": emoji, "add": !mine])
        }
        await reloadMessages(for: ref)
    }

    func deleteMessage(_ message: Message, forEveryone: Bool) async {
        guard let ref = selection else { return }
        await perform {
            try await Core.run("deleteItems", ["chat": ref, "itemIds": [message.id], "forEveryone": forEveryone])
        }
        await reloadMessages(for: ref)
    }

    func download(_ file: FileAttachment, approved: Bool = false) async {
        do {
            let result = try await Core.call("receiveFile", ["fileId": file.id, "approved": approved], as: ReceiveResult.self)
            if !result.accepted {
                let alert = NSAlert()
                alert.messageText = "Download from unknown servers?"
                alert.informativeText = "This file is stored on servers you don't use: "
                    + result.servers.joined(separator: ", ")
                    + ". They will see your IP address."
                alert.addButton(withTitle: "Download")
                alert.addButton(withTitle: "Cancel")
                if alert.runModal() == .alertFirstButtonReturn {
                    await download(file, approved: true)
                }
            }
        } catch {
            report(error)
        }
    }

    /// A readable local copy of an attachment (decrypted when stored encrypted).
    func readablePath(_ file: FileAttachment) async -> URL? {
        guard let path = file.path else { return nil }
        var params: [String: Any] = ["path": path]
        if let key = file.key, let nonce = file.nonce {
            params["key"] = key
            params["nonce"] = nonce
        }
        guard let plain = try? await Core.call("plainFile", params, as: String.self) else { return nil }
        return URL(fileURLWithPath: plain)
    }

    func open(_ file: FileAttachment) async {
        guard let url = await readablePath(file) else { return }
        NSWorkspace.shared.open(url)
    }

    func saveAs(_ file: FileAttachment) async {
        guard let url = await readablePath(file) else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = file.name
        guard panel.runModal() == .OK, let target = panel.url else { return }
        do {
            if FileManager.default.fileExists(atPath: target.path) {
                try FileManager.default.removeItem(at: target)
            }
            try FileManager.default.copyItem(at: url, to: target)
        } catch {
            report(error)
        }
    }

    // MARK: - Contacts

    func createInvitation() async -> ConnLink? {
        do {
            return try await Core.call("createInvitation", as: ConnLink.self)
        } catch {
            report(error)
            return nil
        }
    }

    func addressLink() async -> ConnLink? {
        do {
            return try await Core.call("addressLink", as: ConnLink.self)
        } catch {
            report(error)
            return nil
        }
    }

    func connect(link: String) async -> Bool {
        do {
            try await Core.run("connect", ["link": link])
            scheduleChatsReload()
            return true
        } catch {
            report(error)
            return false
        }
    }

    func createGroup(name: String) async {
        do {
            let ref = try await Core.call("createGroup", ["name": name], as: String.self)
            await reloadChats()
            destination = .chats
            selection = ref
        } catch {
            report(error)
        }
    }

    // MARK: - Profile & settings

    func updateProfile(displayName: String, fullName: String, bio: String, image: String??) async {
        var params: [String: Any] = ["displayName": displayName, "fullName": fullName, "shortDescr": bio]
        if let image = image {
            params["image"] = image ?? ""
        }
        do {
            user = try await Core.call("updateProfile", params, as: User.self)
        } catch {
            report(error)
        }
    }

    func setDeliveryReceipts(groups: Bool, on: Bool) async {
        do {
            user = try await Core.call("setDeliveryReceipts", ["groups": groups, "on": on], as: User.self)
        } catch {
            report(error)
        }
    }

    func setEncryptLocalFiles(_ on: Bool) async {
        await perform { try await Core.run("setEncryptLocalFiles", ["on": on]) }
        info = try? await Core.call("appInfo", as: AppInfo.self)
    }

    // MARK: - Events

    private func handle(_ event: CoreEvent) {
        guard phase == .ready else { return }
        if let p = event.progress, p.total > 0 {
            fileProgress[p.fileId] = Double(p.transferred) / Double(p.total)
        }
        if event.refreshChats { scheduleChatsReload() }
        if event.refreshMessages, let sel = selection, event.chat == nil || event.chat == sel {
            scheduleMessagesReload()
        }
        for message in event.incoming {
            notify(message)
        }
        if event.type == "newChatItems", let chat = event.chat, chat == selection, NSApp.isActive {
            Task { await markRead(chat) }
        }
    }

    private func notify(_ message: IncomingMessage) {
        guard notificationsEnabled else { return }
        if NSApp.isActive && message.chat == selection { return }
        if chats.first(where: { $0.ref == message.chat })?.muted == true { return }
        var body = "New message"
        if !notificationsHideText && !message.text.isEmpty {
            body = message.text
            if let sender = message.sender { body = "\(sender): \(body)" }
        }
        Notifier.post(title: message.title, body: body, chat: message.chat)
    }

    // MARK: - Helpers

    private func perform(_ work: () async throws -> Void) async {
        do {
            try await work()
            scheduleChatsReload()
        } catch {
            report(error)
        }
    }

    func report(_ error: Error) {
        errorMessage = error.localizedDescription
    }
}
