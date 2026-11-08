import Foundation

// Mirrors the JSON built in macos/bridge/src/dto.rs.

struct AppInfo: Decodable {
    let setupComplete: Bool
    let databaseExists: Bool
    let displayName: String?
    let encryptLocalFiles: Bool
    let dataDir: String
    let version: String
}

struct User: Decodable, Equatable {
    let userId: Int64
    let displayName: String
    let fullName: String
    let shortDescr: String
    let description: String
    let image: String?
    let contactLink: String?
    let sendRcptsContacts: Bool
    let sendRcptsSmallGroups: Bool
}

struct ConnLink: Decodable, Equatable {
    let display: String
    let full: String
    let short: String?
}

struct Chat: Decodable, Identifiable, Equatable {
    let ref: String
    let type: String
    let apiId: Int64
    let title: String
    let preview: String
    let unread: Int
    let unreadChat: Bool
    let notifications: String
    let favorite: Bool
    let pendingRequestId: Int64?
    let image: String?
    /// Public name (title is the local alias when one is set).
    let name: String?
    let alias: String?
    let fullName: String
    let shortDescr: String
    let description: String?
    let contactLink: String?
    let isChannel: Bool
    let role: String?
    let lastTs: String?
    /// Per-chat delivery receipts: nil = your default.
    let sendReceipts: Bool?
    /// Delete messages on this Mac after N seconds (nil = app default).
    let chatItemTtl: Int64?
    let tagIds: [Int64]
    let contactPrefs: ContactPrefs?
    let groupPrefs: GroupPrefs?
    let memberReview: Bool
    let membersRequireAttention: Int

    var id: String { ref }
    var muted: Bool { notifications != "all" }
    var hasUnread: Bool { unread > 0 || unreadChat }
    var isGroup: Bool { type == "group" }
    var isNotes: Bool { type == "local" }
    var lastDate: Date? { lastTs.flatMap(Dates.parse) }
    var isDirect: Bool { type == "direct" }
    /// Own role in a group, ranked (0 when not a member / not a group).
    var roleRank: Int { role.map(groupRoleRank) ?? 0 }
    var isOwner: Bool { roleRank >= groupRoleRank("owner") }
    var isAdmin: Bool { roleRank >= groupRoleRank("admin") }
    var isModerator: Bool { roleRank >= groupRoleRank("moderator") }
}

/// One contact feature: what you allow, what they allow, and whether it
/// works in this chat. Allow values: "no", "yes", "always".
struct ContactFeature: Decodable, Equatable {
    let mine: String
    let contact: String
    let enabled: Bool
}

struct ContactPrefs: Decodable, Equatable {
    let features: [String: ContactFeature]
    /// Disappearing messages time, seconds.
    let timedTtl: Int64
}

struct GroupFeature: Decodable, Equatable {
    let on: Bool
}

struct GroupPrefs: Decodable, Equatable {
    let features: [String: GroupFeature]
    let timedTtl: Int64
}

/// A chat list ("Work", "Family"…).
struct ChatTag: Decodable, Identifiable, Equatable {
    let id: Int64
    let text: String
    let emoji: String?
}

struct CreatedChannel: Decodable {
    let ref: String
    let link: String?
}

/// Group role rank (higher = more rights); unknown / none = 0. A free
/// function so non-UI code can use it (AppModel is main-actor isolated).
func groupRoleRank(_ role: String) -> Int {
    ["observer", "member", "moderator", "admin", "owner"].firstIndex(of: role).map { $0 + 1 } ?? 0
}

/// A chat feature row: preference key, label, icon.
struct FeatureInfo: Identifiable {
    let key: String
    let label: String
    let icon: String
    var id: String { key }

    init(_ key: String, _ label: String, _ icon: String) {
        self.key = key
        self.label = label
        self.icon = icon
    }
}

/// Labels and choices shared by the preference pages.
enum Features {
    static let contact: [FeatureInfo] = [
        FeatureInfo("timedMessages", "Disappearing messages", "timer"),
        FeatureInfo("fullDelete", "Delete for everyone", "trash-delete"),
        FeatureInfo("reactions", "Message reactions", "reaction-emoji-add"),
        FeatureInfo("voice", "Voice messages", "mic"),
        FeatureInfo("calls", "Audio and video calls", "calls"),
    ]

    static let group: [FeatureInfo] = [
        FeatureInfo("timedMessages", "Disappearing messages", "timer"),
        FeatureInfo("directMessages", "Direct messages", "paperplane"),
        FeatureInfo("fullDelete", "Delete for everyone", "trash-delete"),
        FeatureInfo("reactions", "Message reactions", "reaction-emoji-add"),
        FeatureInfo("voice", "Voice messages", "mic"),
        FeatureInfo("files", "Files and media", "image-square"),
        FeatureInfo("simplexLinks", "SimpleX links", "link-minimalistic-2"),
        FeatureInfo("history", "Recent history for new members", "history"),
        FeatureInfo("support", "Chat with admins", "admin"),
    ]

    /// Disappearing-message times (seconds).
    static let timedChoices: [Int64] = [30, 300, 3_600, 86_400, 604_800, 2_592_000]

    /// "Delete messages after" on this device; nil = default (never).
    static let deleteAfterChoices: [Int64?] = [nil, 86_400, 604_800, 2_592_000, 31_536_000]

    static func duration(_ secs: Int64) -> String {
        switch secs {
        case 30: return "30 seconds"
        case 300: return "5 minutes"
        case 3_600: return "1 hour"
        case 86_400: return "1 day"
        case 604_800: return "1 week"
        case 2_592_000: return "1 month"
        case 31_536_000: return "1 year"
        case ..<60: return "\(secs) seconds"
        case ..<3_600: return "\(secs / 60) minutes"
        case ..<86_400: return "\(secs / 3_600) hours"
        default: return "\(secs / 86_400) days"
        }
    }

    static func allowLabel(_ value: String) -> String {
        switch value {
        case "always": return "Always"
        case "yes": return "Yes"
        default: return "No"
        }
    }
}

struct Segment: Decodable, Equatable {
    let text: String
    let style: String
    let color: String?
    let link: String?
    let simplex: String?
}

struct FileAttachment: Decodable, Equatable {
    let id: Int64
    let name: String
    let size: Int64
    let state: String
    let kind: String
    let duration: Int?
    let path: String?
    let key: String?
    let nonce: String?
    let progress: Double?
    let error: String?
    /// Downloaded without asking (core rule; Settings → auto-download).
    let autoReceive: Bool?

    var isComplete: Bool { state == "complete" && path != nil }
}

struct Reaction: Decodable, Equatable, Hashable {
    let emoji: String
    let glyph: String
    let mine: Bool
    let total: Int
}

struct Quote: Decodable, Equatable {
    let itemId: Int64?
    let text: String
    let outgoing: Bool
}

struct Sender: Decodable, Equatable {
    let memberId: Int64
    let contactId: Int64?
    let name: String
    let image: String?
    let role: String
}

struct Member: Decodable, Identifiable, Equatable {
    let id: Int64
    let contactId: Int64?
    let name: String
    let publicName: String
    let role: String
    let status: String
    let image: String?
    let shortDescr: String?
    let isMe: Bool
    let blocked: Bool
    /// False when you hid this member's messages on this Mac.
    let showMessages: Bool

    /// Invited / still connecting: shown with the gold pending clock.
    var pending: Bool {
        ["invited", "intro-inv", "pending_approval", "pending_review"].contains(status.lowercased())
    }

    /// Waiting for a moderator to let them in.
    var awaitingReview: Bool {
        ["pending_approval", "pending_review"].contains(status.lowercased())
    }

    /// Left, removed or the group was deleted — no actions apply.
    var gone: Bool {
        ["left", "removed", "deleted", "group_deleted"].contains(status.lowercased())
    }
}

struct GroupInvite: Decodable, Equatable {
    let groupId: Int64
    let name: String
    let role: String
    let status: String
    let pending: Bool
}

struct Message: Decodable, Identifiable, Equatable {
    let id: Int64
    let text: String
    let segments: [Segment]
    let outgoing: Bool
    let createdAt: String?
    let delivery: String
    let file: FileAttachment?
    let reactions: [Reaction]
    let quote: Quote?
    let deleted: String?
    let sender: Sender?
    let announcement: Bool
    let groupInvitation: GroupInvite?
    let edited: Bool
    let canEdit: Bool
    let emojiOnly: Bool

    var date: Date? { createdAt.flatMap(Dates.parse) }
    var hasSecret: Bool { segments.contains { $0.style == "secret" } }
}

/// Message details (`messageInfo`): time, delivery status, text.
struct MessageInfo: Decodable {
    let time: String?
    let status: String?
    let text: String

    var date: Date? { time.flatMap(Dates.parse) }

    /// Plain words for the core's `itemStatus.type`.
    var statusLabel: String? {
        guard let status = status else { return nil }
        switch status {
        case "sndNew": return "Sending"
        case "sndSent": return "Sent"
        case "sndRcvd": return "Delivered"
        case "sndErrorAuth", "sndError": return "Not delivered"
        case "sndWarning": return "Delivery warning"
        case "rcvNew": return "Received"
        case "rcvRead": return "Read"
        default: return status
        }
    }
}

struct FileProgress: Decodable {
    let fileId: Int64
    let transferred: Int64
    let total: Int64
}

struct IncomingMessage: Decodable {
    let chat: String
    let title: String
    let sender: String?
    let text: String
}

struct CoreEvent: Decodable {
    let type: String
    let refreshChats: Bool
    let refreshMessages: Bool
    let chat: String?
    let progress: FileProgress?
    let incoming: [IncomingMessage]
}

/// A `:shortcode` suggestion for the composer.
struct EmojiHit: Decodable, Hashable {
    let shortcode: String
    let glyph: String
}

/// A clicked link after the core's checks: `url` nil = can't be opened;
/// `simplex` set = a SimpleX link (connect instead of opening a browser).
struct PreparedLink: Decodable {
    let url: String?
    let simplex: String?
}

struct ReceiveResult: Decodable {
    let accepted: Bool
    let servers: [String]
}

struct SelfTestResult: Decodable {
    let libsDir: String
    let user: String
    let chats: Int
}

/// Protocol reaction set (`KNOWN_REACTIONS` in the core); ❤ is sent bare.
struct ReactionOption: Hashable {
    let emoji: String
    let glyph: String
}

let knownReactions: [ReactionOption] = [
    ReactionOption(emoji: "👍", glyph: "👍"), ReactionOption(emoji: "👎", glyph: "👎"),
    ReactionOption(emoji: "😀", glyph: "😀"), ReactionOption(emoji: "😂", glyph: "😂"),
    ReactionOption(emoji: "😢", glyph: "😢"), ReactionOption(emoji: "❤", glyph: "❤️"),
    ReactionOption(emoji: "🚀", glyph: "🚀"), ReactionOption(emoji: "✅", glyph: "✅"),
]

enum Dates {
    private static let plain = ISO8601DateFormatter()

    /// Core timestamps carry up to nanoseconds ("…14.904358283Z"), more than
    /// ISO8601DateFormatter accepts; minutes are all the UI shows, so drop
    /// the fraction.
    static func parse(_ raw: String) -> Date? {
        guard let dot = raw.firstIndex(of: ".") else { return plain.date(from: raw) }
        let rest = raw[raw.index(after: dot)...]
        let zone = rest.firstIndex { !$0.isNumber } ?? rest.endIndex
        return plain.date(from: String(raw[..<dot]) + String(rest[zone...]))
    }

    private static let time: DateFormatter = {
        let f = DateFormatter()
        f.timeStyle = .short
        f.dateStyle = .none
        return f
    }()
    private static let day: DateFormatter = {
        let f = DateFormatter()
        f.setLocalizedDateFormatFromTemplate("d MMM")
        return f
    }()

    /// Chat list stamp: time today, else a short date.
    static func listStamp(_ date: Date) -> String {
        Calendar.current.isDateInToday(date) ? time.string(from: date) : day.string(from: date)
    }

    static func clock(_ date: Date) -> String {
        time.string(from: date)
    }
}
