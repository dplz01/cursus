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

    /// Cursus icon (macos/Resources/icons).
    var icon: String { rawValue }
}

enum SettingsPage: String, CaseIterable, Identifiable {
    // Linux order.
    case profile, address, privacy, notifications, appearance, about
    var id: String { rawValue }

    var title: String {
        switch self {
        case .profile: return "Profile"
        case .address: return "Address"
        case .privacy: return "Privacy & security"
        case .notifications: return "Notifications"
        case .appearance: return "Appearance"
        case .about: return "About"
        }
    }

    var icon: String {
        switch self {
        case .profile: return "user-alt-1"
        case .address: return "link-minimalistic-2"
        case .privacy: return "shield-check"
        case .notifications: return "sf:bell"
        case .appearance: return "image-square"
        case .about: return "info-circle"
        }
    }

    var subtitle: String {
        switch self {
        case .profile: return "Name, picture and bio"
        case .address: return "How people reach you"
        case .privacy: return "Lock, screen, files and receipts"
        case .notifications: return "Banners for new messages"
        case .appearance: return "Theme, text size and typing"
        case .about: return "Version, data and licences"
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
    /// Contact / group info panel beside the conversation (⌘I).
    @Published var showInfo = false
    @Published var members: [Member] = []
    /// Chat lists (tags) for "Add to list".
    @Published var chatTags: [ChatTag] = []

    // Message actions in the open chat.
    /// Selecting several messages (forward / delete together).
    @Published var selectingMessages = false
    @Published var selectedMessages: Set<Int64> = []
    /// Messages whose secret text or "marked deleted" original is shown.
    @Published var revealedMessages: Set<Int64> = []
    /// Message ids being forwarded (opens the chat picker).
    @Published var forwarding: [Int64]?
    /// Message whose info sheet is open.
    @Published var infoMessage: Message?
    /// Scroll to this message (jump to a quoted one), then briefly highlight it.
    @Published var scrollTarget: Int64?
    @Published var highlightedMessage: Int64?
    private var highlightToken = 0
    /// "New messages" divider above this message (Linux: 5 s after new
    /// incoming messages, 10 s when the chat opens with unread ones).
    @Published var newMessagesFrom: Int64?
    private var newMessagesToken = 0
    /// Unread count of the chat being opened (its badge clears at once).
    private var openUnread = 0

    // Media.
    let recorder = VoiceRecorder()
    /// A finished recording waiting to be sent or discarded.
    @Published var voiceDraft: VoiceDraft?
    /// The photo / video gallery of the open chat.
    @Published var gallery: GalleryState?
    /// An open "Chat with admins" thread of the selected group.
    @Published var support: SupportThread?
    /// The profile page to show when it opens next (back from a thread).
    @Published var infoReturnPage: InfoPage?
    @Published var isLoadingMessages = false

    /// Settings → Notifications (`@AppStorage` with these keys in the view).
    static let notificationsEnabledKey = "notificationsEnabled"
    static let notificationsHideTextKey = "notificationsHideText"
    private var notificationsEnabled: Bool {
        UserDefaults.standard.object(forKey: Self.notificationsEnabledKey) as? Bool ?? true
    }
    /// On by default (Linux too): banners say only "New message".
    private var notificationsHideText: Bool {
        UserDefaults.standard.object(forKey: Self.notificationsHideTextKey) as? Bool ?? true
    }

    private var pump: EventPump?
    private lazy var idleLock = IdleLock { [weak self] in
        guard let self = self, self.phase == .ready else { return }
        self.lock()
    }
    /// Files already asked for (auto-download), so each is requested once.
    private var autoReceived: Set<Int64> = []
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
        // Once per launch. Closing the window keeps Cursus (and the open
        // database) running; the reopened window must not start over —
        // that showed the unlock screen and dropped the saved password.
        guard phase == .starting else { return }
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
            } else if !quiet {
                Keychain.delete()
            }
            info = try? await Core.call("appInfo", as: AppInfo.self)
            phase = .ready
            idleLock.start()
            Notifier.requestAuthorization()
            await reloadChats()
        } catch {
            let text = error.localizedDescription
            let wrongKey = text.lowercased().contains("wrong") || text.lowercased().contains("passphrase")
                || text.lowercased().contains("key")
            if quiet {
                // Only a password the database rejects is stale; anything
                // else (busy, already open) keeps it for next time.
                if wrongKey { Keychain.delete() }
            } else if wrongKey {
                errorMessage = "Wrong password.\n\n\(text)"
            } else {
                errorMessage = text
            }
        }
    }

    func lock() {
        // Unlock waits (busy) until the database is really closed.
        busy = true
        Task {
            try? await Core.run("close")
            busy = false
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
        members = []
        fileProgress = [:]
        // The profile is a full page: another chat opens on its messages.
        showInfo = false
        selectingMessages = false
        selectedMessages = []
        revealedMessages = []
        highlightedMessage = nil
        newMessagesFrom = nil
        cancelVoice()
        gallery = nil
        support = nil
        openUnread = selection.flatMap { ref in chats.first { $0.ref == ref }?.unread } ?? 0
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
            guard activeRef == ref else { return }
            let old = messages
            if list != messages { messages = list }
            markNewMessages(old: old, new: list)
            autoDownload(list)
        } catch {
            report(error)
        }
    }

    func scheduleMessagesReload() {
        guard let ref = activeRef else { return }
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

    // The bridge keeps the settings a call leaves out.
    func setFavorite(_ chat: Chat, _ on: Bool) async {
        await perform { try await Core.run("setChatSettings", ["chat": chat.ref, "favorite": on]) }
    }

    func setMuted(_ chat: Chat, _ muted: Bool) async {
        await setNotifications(chat, muted ? "none" : "all")
    }

    /// "all", "mentions" (groups) or "none".
    func setNotifications(_ chat: Chat, _ mode: String) async {
        await perform { try await Core.run("setChatSettings", ["chat": chat.ref, "notifications": mode]) }
    }

    /// nil = follow your default (Settings → Privacy).
    func setSendReceipts(_ chat: Chat, _ on: Bool?) async {
        let value: Any = on.map { $0 as Any } ?? NSNull()
        await perform { try await Core.run("setChatSettings", ["chat": chat.ref, "sendReceipts": value]) }
    }

    /// Delete messages on this Mac after `seconds` (nil = never / default).
    func setChatTtl(_ chat: Chat, _ seconds: Int64?) async {
        let value: Any = seconds.map { $0 as Any } ?? NSNull()
        await perform { try await Core.run("setChatTtl", ["chat": chat.ref, "seconds": value]) }
    }

    /// Your setting for one contact feature ("no" / "yes" / "always").
    func setContactFeature(_ chat: Chat, _ key: String, _ allow: String) async {
        await perform {
            try await Core.run("setContactPrefs", ["chat": chat.ref, "changes": [key: allow]])
        }
    }

    func setContactTimedTtl(_ chat: Chat, _ seconds: Int64) async {
        await perform {
            try await Core.run("setContactPrefs", [
                "chat": chat.ref, "changes": ["timedMessages": "yes", "timedTtl": seconds] as [String: Any],
            ])
        }
    }

    // MARK: - Lists

    func loadChatTags() async {
        do {
            chatTags = try await Core.call("chatTags", as: [ChatTag].self)
        } catch {
            report(error)
        }
    }

    func createChatTag(_ text: String, adding chat: Chat) async {
        let name = text.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        do {
            chatTags = try await Core.call("createChatTag", ["text": name], as: [ChatTag].self)
            if let tag = chatTags.first(where: { $0.text == name }) {
                await setTags(chat, chat.tagIds + [tag.id])
            }
        } catch {
            report(error)
        }
    }

    func toggleTag(_ chat: Chat, _ tag: ChatTag) async {
        let ids = chat.tagIds.contains(tag.id) ? chat.tagIds.filter { $0 != tag.id } : chat.tagIds + [tag.id]
        await setTags(chat, ids)
    }

    private func setTags(_ chat: Chat, _ ids: [Int64]) async {
        await perform { try await Core.run("setChatTags", ["chat": chat.ref, "tagIds": ids]) }
    }

    func clear(_ chat: Chat) async {
        await perform { try await Core.run("clearChat", ["chat": chat.ref]) }
        if selection == chat.ref { await reloadMessages(for: chat.ref) }
    }

    func delete(_ chat: Chat) async {
        await perform { try await Core.run("deleteChat", ["chat": chat.ref]) }
        if selection == chat.ref {
            selection = nil
            showInfo = false
        }
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

    /// ⌃Tab / ⌃⇧Tab: next / previous conversation in the visible list.
    func selectAdjacent(_ step: Int) {
        let list = visibleChats
        guard !list.isEmpty else { return }
        guard let sel = selection, let i = list.firstIndex(where: { $0.ref == sel }) else {
            selection = list.first?.ref
            return
        }
        selection = list[(i + step + list.count) % list.count].ref
    }

    func toggleInfo() {
        guard selectedChat != nil else { return }
        showInfo.toggle()
        if showInfo { Task { await loadMembers() } }
    }

    func loadMembers() async {
        guard let chat = selectedChat, chat.isGroup else {
            members = []
            return
        }
        do {
            let list = try await Core.call("listMembers", ["groupId": chat.apiId], as: [Member].self)
            guard selection == chat.ref else { return }
            members = list.sorted { a, b in
                if a.isMe != b.isMe { return a.isMe }
                let ra = Self.roleRank(a.role), rb = Self.roleRank(b.role)
                if ra != rb { return ra > rb }
                return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
            }
        } catch {
            report(error)
        }
    }

    nonisolated static func roleRank(_ role: String) -> Int {
        groupRoleRank(role)
    }

    func setAlias(_ chat: Chat, _ alias: String) async {
        let trimmed = alias.trimmingCharacters(in: .whitespaces)
        guard trimmed != (chat.alias ?? "") else { return }
        await perform { try await Core.run("setAlias", ["chat": chat.ref, "alias": trimmed]) }
    }

    // MARK: - Group settings

    /// Owners: change some of name / bio / welcome message / picture
    /// (`image: .some(nil)` removes the picture).
    func updateGroupProfile(_ chat: Chat, name: String? = nil, shortDescr: String? = nil,
                            welcome: String? = nil, image: String?? = nil) async {
        var params: [String: Any] = ["groupId": chat.apiId]
        if let name = name { params["name"] = name }
        if let shortDescr = shortDescr { params["shortDescr"] = shortDescr }
        if let welcome = welcome { params["welcome"] = welcome }
        if let image = image { params["image"] = image ?? "" }
        await perform { try await Core.run("updateGroupProfile", params) }
    }

    func setGroupFeature(_ chat: Chat, _ key: String, on: Bool, ttl: Int64? = nil) async {
        var params: [String: Any] = ["groupId": chat.apiId, "key": key, "on": on]
        if let ttl = ttl { params["ttl"] = ttl }
        await perform { try await Core.run("setGroupPref", params) }
    }

    func setMemberReview(_ chat: Chat, _ on: Bool) async {
        await perform { try await Core.run("setMemberReview", ["groupId": chat.apiId, "on": on]) }
    }

    /// The group / channel link (created the first time). Joiners get `role`.
    /// `quiet`: no error alert (subscribers may simply have no link to show).
    func groupLink(_ chat: Chat, role: String = "member", quiet: Bool = false) async -> ConnLink? {
        do {
            return try await Core.call("groupLink", ["groupId": chat.apiId, "role": role], as: ConnLink.self)
        } catch {
            if !quiet { report(error) }
            return nil
        }
    }

    func setGroupLinkRole(_ chat: Chat, _ role: String) async {
        await perform { try await Core.run("setGroupLinkRole", ["groupId": chat.apiId, "role": role]) }
    }

    func deleteGroupLink(_ chat: Chat) async {
        await perform { try await Core.run("deleteGroupLink", ["groupId": chat.apiId]) }
    }

    /// Contacts you can invite (direct chats that are connected contacts).
    var invitableContacts: [Chat] {
        let inGroup = Set(members.compactMap { $0.gone ? nil : $0.contactId })
        return chats.filter { $0.isDirect && $0.pendingRequestId == nil && !inGroup.contains($0.apiId) }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }

    func addMember(_ chat: Chat, contact: Chat, role: String) async {
        await perform {
            try await Core.run("addMember", ["groupId": chat.apiId, "contactId": contact.apiId, "role": role])
        }
        await loadMembers()
    }

    func setMemberRole(_ chat: Chat, _ member: Member, _ role: String) async {
        await perform {
            try await Core.run("setMemberRole", ["groupId": chat.apiId, "memberId": member.id, "role": role])
        }
        await loadMembers()
    }

    func removeMember(_ chat: Chat, _ member: Member, withMessages: Bool) async {
        await perform {
            try await Core.run("removeMember", [
                "groupId": chat.apiId, "memberId": member.id, "withMessages": withMessages,
            ])
        }
        await loadMembers()
    }

    /// Moderators: block for everyone.
    func blockMember(_ chat: Chat, _ member: Member, _ blocked: Bool) async {
        await perform {
            try await Core.run("blockMember", ["groupId": chat.apiId, "memberId": member.id, "blocked": blocked])
        }
        await loadMembers()
    }

    /// Hide / show a member's messages on this Mac.
    func setMemberShowMessages(_ chat: Chat, _ member: Member, _ show: Bool) async {
        await perform {
            try await Core.run("setMemberShowMessages", ["groupId": chat.apiId, "memberId": member.id, "show": show])
        }
        await loadMembers()
        if selection == chat.ref { await reloadMessages(for: chat.ref) }
    }

    func acceptMember(_ chat: Chat, _ member: Member, role: String = "member") async {
        await perform {
            try await Core.run("acceptMember", ["groupId": chat.apiId, "memberId": member.id, "role": role])
        }
        await loadMembers()
        refreshSupportMember()
    }

    /// The open thread's member after a change (accepted, removed).
    private func refreshSupportMember() {
        guard let thread = support, let old = thread.member else { return }
        let fresh = members.first { $0.id == old.id }
        support = SupportThread(groupRef: thread.groupRef, groupId: thread.groupId, member: fresh ?? old,
                                title: thread.title, fromList: thread.fromList)
    }

    /// Open the direct chat with a member, creating it when the group
    /// allows direct messages.
    func messageMember(_ chat: Chat, _ member: Member) async {
        if let id = member.contactId, chats.contains(where: { $0.ref == "@\(id)" }) {
            openContact(id)
            return
        }
        do {
            let ref = try await Core.call("memberContact", ["groupId": chat.apiId, "memberId": member.id], as: String.self)
            await reloadChats()
            destination = .chats
            selection = ref
        } catch {
            report(error)
        }
    }

    func leaveGroup(_ chat: Chat) async {
        await perform { try await Core.run("leaveGroup", ["groupId": chat.apiId]) }
        if selection == chat.ref { await loadMembers() }
    }

    /// Open (or start) the direct chat with a group member.
    func openContact(_ contactId: Int64) {
        let ref = "@\(contactId)"
        guard chats.contains(where: { $0.ref == ref }) else { return }
        destination = .chats
        selection = ref
    }

    func joinGroup(_ groupId: Int64) async {
        await perform { try await Core.run("joinGroup", ["groupId": groupId]) }
    }

    // MARK: - Support threads ("Chat with admins")

    /// The conversation shown: the selected chat, or one of its support
    /// threads (`#group(_support)`, `#group(_support:member)`).
    var activeRef: String? { support?.ref ?? selection }

    /// `#9(_support:4)` → `#9`.
    nonisolated static func baseRef(_ ref: String) -> String {
        ref.split(separator: "(").first.map(String.init) ?? ref
    }

    /// Your own chat with the group's admins, or (moderators) a member's.
    func openSupport(_ chat: Chat, member: Member? = nil, fromList: Bool = false) {
        guard chat.isGroup else { return }
        let title = member.map { "Chat with \($0.name)" } ?? "Chat with admins"
        support = SupportThread(groupRef: chat.ref, groupId: chat.apiId, member: member,
                                title: title, fromList: fromList)
        showInfo = false
        messages = []
        composeMode = .new
        stopSelecting()
        guard let ref = activeRef else { return }
        isLoadingMessages = true
        Task {
            await reloadMessages(for: ref)
            isLoadingMessages = false
            try? await Core.run("markRead", ["chat": ref])
            scheduleChatsReload()
        }
    }

    /// Back from a thread: to the moderators' list it came from, else the group.
    func closeSupport() {
        let fromList = support?.fromList ?? false
        support = nil
        messages = []
        guard let ref = selection else { return }
        Task { await reloadMessages(for: ref) }
        if fromList {
            infoReturnPage = .supportList
            showInfo = true
            Task { await loadMembers() }
        }
    }

    /// Moderators: close a member's support thread.
    func deleteSupportChat(_ chat: Chat, _ member: Member) async {
        await perform {
            try await Core.run("deleteSupportChat", ["groupId": chat.apiId, "memberId": member.id])
        }
        await loadMembers()
    }

    // MARK: - Messages

    func send() async {
        guard let ref = activeRef else { return }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        let mode = composeMode
        draft = ""
        composeMode = .new
        do {
            // "Remove tracking from links" cleans what you send (core rule).
            let clean = Prefs.cleanLinksOn
            switch mode {
            case .edit(let message):
                try await Core.run("updateText", ["chat": ref, "itemId": message.id, "text": text, "cleanLinks": clean])
            case .reply(let message):
                try await Core.run("sendText", ["chat": ref, "text": text, "quote": message.id, "cleanLinks": clean])
            case .new:
                try await Core.run("sendText", ["chat": ref, "text": text, "cleanLinks": clean])
            }
            await reloadMessages(for: ref)
            scheduleChatsReload()
        } catch {
            draft = text
            composeMode = mode
            report(error)
        }
    }

    /// The composer's text for a chat it is leaving.
    func saveDraft(_ text: String, for ref: String) {
        guard ref != selection else { return }
        drafts[ref] = text.isEmpty ? nil : text
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
        guard let ref = activeRef else { return }
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
        guard let ref = activeRef else { return }
        let mine = message.reactions.contains { $0.emoji == emoji && $0.mine }
        await perform {
            try await Core.run("react", ["chat": ref, "itemId": message.id, "emoji": emoji, "add": !mine])
        }
        await reloadMessages(for: ref)
    }

    func deleteMessage(_ message: Message, forEveryone: Bool) async {
        guard let ref = activeRef else { return }
        await perform {
            try await Core.run("deleteItems", ["chat": ref, "itemIds": [message.id], "forEveryone": forEveryone])
        }
        await reloadMessages(for: ref)
    }

    // MARK: - Message actions

    func startSelecting(_ message: Message) {
        selectingMessages = true
        selectedMessages = [message.id]
    }

    func toggleSelected(_ message: Message) {
        if selectedMessages.contains(message.id) {
            selectedMessages.remove(message.id)
        } else {
            selectedMessages.insert(message.id)
        }
    }

    func stopSelecting() {
        selectingMessages = false
        selectedMessages = []
    }

    /// Delete the selected messages from this Mac.
    func deleteSelected() async {
        guard let ref = activeRef, !selectedMessages.isEmpty else { return }
        let ids = Array(selectedMessages)
        stopSelecting()
        await perform { try await Core.run("deleteItems", ["chat": ref, "itemIds": ids, "forEveryone": false]) }
        await reloadMessages(for: ref)
    }

    func forwardSelected() {
        guard !selectedMessages.isEmpty else { return }
        forwarding = messages.filter { selectedMessages.contains($0.id) }.map(\.id)
    }

    /// Forward `ids` from the open chat to `target`.
    func forward(_ ids: [Int64], to target: Chat) async {
        guard let ref = activeRef, !ids.isEmpty else { return }
        forwarding = nil
        stopSelecting()
        await perform { try await Core.run("forward", ["chat": ref, "to": target.ref, "itemIds": ids]) }
        if target.ref == ref { await reloadMessages(for: ref) }
    }

    func messageInfo(_ message: Message) async -> MessageInfo? {
        guard let ref = activeRef else { return nil }
        do {
            return try await Core.call("messageInfo", ["chat": ref, "itemId": message.id], as: MessageInfo.self)
        } catch {
            report(error)
            return nil
        }
    }

    /// Groups: names of the members who set `emoji` on `message`.
    func reactionMembers(_ message: Message, emoji: String) async -> [String] {
        guard let ref = activeRef else { return [] }
        return (try? await Core.call("reactionMembers", ["chat": ref, "itemId": message.id, "emoji": emoji],
                                     as: [String].self)) ?? []
    }

    func toggleRevealed(_ message: Message) {
        if revealedMessages.contains(message.id) {
            revealedMessages.remove(message.id)
        } else {
            revealedMessages.insert(message.id)
        }
    }

    /// Scroll to a quoted message and highlight it for a moment.
    func jumpTo(_ itemId: Int64?) {
        guard let id = itemId, messages.contains(where: { $0.id == id }) else {
            errorMessage = "The original message is no longer available."
            return
        }
        scrollTarget = id
        highlightedMessage = id
        highlightToken += 1
        let token = highlightToken
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: 1_600_000_000)
            guard let self = self, self.highlightToken == token else { return }
            self.highlightedMessage = nil
        }
    }

    /// Opening a chat with unread messages: mark the oldest of them. Later
    /// loads: mark the first new incoming message (the view scrolls only for
    /// your own messages, or when you're already at the bottom).
    private func markNewMessages(old: [Message], new: [Message]) {
        if old.isEmpty {
            let unread = openUnread
            openUnread = 0
            guard unread > 0 else { return }
            let incoming = new.filter { !$0.outgoing && !$0.announcement }
            if let first = incoming.suffix(unread).first { showNewMessages(from: first.id, seconds: 10) }
            return
        }
        guard let lastOld = old.last?.id, let at = new.lastIndex(where: { $0.id == lastOld }) else { return }
        let tail = new[new.index(after: at)...]
        if let first = tail.first(where: { !$0.outgoing && !$0.announcement }) {
            showNewMessages(from: newMessagesFrom ?? first.id, seconds: 5)
        }
    }

    private func showNewMessages(from id: Int64, seconds: Double) {
        newMessagesFrom = id
        newMessagesToken += 1
        let token = newMessagesToken
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            guard let self = self, self.newMessagesToken == token else { return }
            self.newMessagesFrom = nil
        }
    }

    // MARK: - Voice messages

    func startRecording() async {
        guard activeRef != nil, voiceDraft == nil else { return }
        VoicePlayer.shared.stop()
        do {
            try await recorder.start()
        } catch {
            report(error)
        }
    }

    func stopRecording() {
        guard recorder.recording else { return }
        if let draft = recorder.stop() {
            voiceDraft = draft
        } else {
            errorMessage = "That recording was too short."
        }
    }

    /// Discard the recording or the draft.
    func cancelVoice() {
        if recorder.recording { recorder.cancel() }
        if let draft = voiceDraft {
            try? FileManager.default.removeItem(at: draft.url)
            voiceDraft = nil
        }
    }

    func sendVoiceDraft() async {
        guard let draft = voiceDraft, let ref = activeRef else { return }
        voiceDraft = nil
        do {
            // The core copies (and encrypts) it into the files folder.
            try await Core.run("sendVoice", ["chat": ref, "path": draft.url.path, "duration": draft.duration])
            try? FileManager.default.removeItem(at: draft.url)
            await reloadMessages(for: ref)
            scheduleChatsReload()
        } catch {
            voiceDraft = draft
            report(error)
        }
    }

    // MARK: - Gallery

    /// Open the chat's photos and videos at this message.
    func openGallery(at messageId: Int64) {
        let items = messages.filter { message in
            guard let file = message.file, message.deleted == nil, file.isComplete else { return false }
            return file.kind == "image" || file.kind == "video"
        }
        guard let index = items.firstIndex(where: { $0.id == messageId }) else { return }
        gallery = GalleryState(items: items, index: index)
    }

    /// ↑ in an empty composer: edit your latest editable message.
    func editLastOutgoing() -> Bool {
        guard let message = messages.last(where: { $0.outgoing && $0.canEdit && $0.deleted == nil }) else { return false }
        startEdit(message)
        return true
    }

    // MARK: - Emoji shortcodes (core rules)

    func emojiSuggestions(for text: String) async -> [EmojiHit] {
        guard text.contains(":") else { return [] }
        return (try? await Core.call("emojiSuggest", ["text": text], as: [EmojiHit].self)) ?? []
    }

    func acceptShortcode(_ text: String, glyph: String) async -> String? {
        do {
            return try await Core.call("acceptShortcode", ["text": text, "glyph": glyph], as: String?.self)
        } catch {
            return nil
        }
    }

    func expandShortcodes(_ text: String) async -> String {
        (try? await Core.call("expandShortcodes", ["text": text], as: String.self)) ?? text
    }

    /// Images, voice and short videos download by themselves (official
    /// caps, decided by the core). With "Protect IP address" on, files on
    /// servers you don't use are skipped; Download asks for them later.
    private func autoDownload(_ list: [Message]) {
        guard Prefs.autoDownloadOn else { return }
        for message in list where !message.outgoing {
            guard let file = message.file, file.autoReceive == true, file.state == "offer",
                  !autoReceived.contains(file.id) else { continue }
            autoReceived.insert(file.id)
            let approved = !Prefs.protectIPOn
            Task { _ = try? await Core.call("receiveFile", ["fileId": file.id, "approved": approved], as: ReceiveResult.self) }
        }
    }

    func download(_ file: FileAttachment, approved: Bool = false) async {
        let approved = approved || !Prefs.protectIPOn
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
        // A sender names the file: never launch something that could run code.
        let risky = (try? await Core.call("riskyFile", ["path": url.path], as: Bool.self)) ?? true
        if risky {
            errorMessage = "This file type could run code, so Cursus won't open it. Use Save As… if you trust it."
            return
        }
        NSWorkspace.shared.open(url)
    }

    /// A link clicked in a message: checked by the core (only web / mail /
    /// phone; tracking codes removed when "Clean links" is on), then opened
    /// after you confirm. SimpleX links offer to connect instead.
    func openLink(_ url: URL) {
        Task {
            let prepared = try? await Core.call(
                "prepareLink", ["url": url.absoluteString, "clean": Prefs.cleanLinksOn], as: PreparedLink.self)
            guard let target = prepared?.url, !target.isEmpty else {
                errorMessage = "This kind of link can't be opened from Cursus."
                return
            }
            let alert = NSAlert()
            if prepared?.simplex != nil {
                alert.messageText = "Connect via this SimpleX link?"
                alert.informativeText = target
                alert.addButton(withTitle: "Connect")
            } else {
                alert.messageText = "Open this link?"
                alert.informativeText = target
                alert.addButton(withTitle: "Open")
            }
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
            if prepared?.simplex != nil {
                _ = await connect(link: target)
            } else if let open = URL(string: target) {
                NSWorkspace.shared.open(open)
            }
        }
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

    /// A channel (relay-backed group); returns its link to share.
    func createChannel(name: String) async -> String? {
        do {
            let created = try await Core.call("createChannel", ["name": name], as: CreatedChannel.self)
            await reloadChats()
            destination = .channels
            selection = created.ref
            return created.link
        } catch {
            report(error)
            return nil
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

    /// Change some of your profile (nil = keep; `image: .some(nil)` removes it).
    func updateProfile(displayName: String? = nil, fullName: String? = nil, bio: String? = nil,
                       description: String? = nil, image: String?? = nil) async {
        var params: [String: Any] = [:]
        if let v = displayName { params["displayName"] = v }
        if let v = fullName { params["fullName"] = v }
        if let v = bio { params["shortDescr"] = v }
        if let v = description { params["description"] = v }
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

    // MARK: - Address

    func addressAutoAccept() async -> Bool {
        (try? await Core.call("addressAutoAccept", as: Bool.self)) ?? false
    }

    func setAddressAutoAccept(_ on: Bool) async -> Bool {
        do {
            return try await Core.call("setAddressAutoAccept", ["on": on], as: Bool.self)
        } catch {
            report(error)
            return !on
        }
    }

    /// Show your address on your profile (contacts can share it).
    func setProfileAddress(_ on: Bool) async {
        do {
            user = try await Core.call("setProfileAddress", ["on": on], as: User.self)
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
        // A support thread's events may carry the scoped or the plain group
        // reference: compare the chat part.
        if event.refreshMessages, let active = activeRef,
           event.chat == nil || event.chat.map(Self.baseRef) == Self.baseRef(active) {
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
