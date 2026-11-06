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
    let fullName: String
    let shortDescr: String
    let isChannel: Bool
    let role: String?
    let lastTs: String?

    var id: String { ref }
    var muted: Bool { notifications != "all" }
    var hasUnread: Bool { unread > 0 || unreadChat }
    var isGroup: Bool { type == "group" }
    var isNotes: Bool { type == "local" }
    var lastDate: Date? { lastTs.flatMap(Dates.parse) }
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
