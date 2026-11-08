import AppKit
import SwiftUI

/// This Mac's app settings (UserDefaults; `@AppStorage` with these keys in
/// views). Same choices as the Linux app's preferences; what they mean for
/// links, files and receipts is decided by the core.
enum Prefs {
    static let theme = "theme"
    static let textSize = "textSize"
    static let enterSends = "enterSends"
    static let lockAfter = "lockAfter"
    static let chatPreviews = "chatPreviews"
    static let blurMedia = "blurMedia"
    static let autoDownload = "autoDownload"
    static let protectIP = "protectIP"
    static let cleanLinks = "cleanLinks"

    private static var defaults: UserDefaults { .standard }

    static func bool(_ key: String, default value: Bool) -> Bool {
        defaults.object(forKey: key) as? Bool ?? value
    }

    static var enterSendsOn: Bool { bool(enterSends, default: true) }
    static var chatPreviewsOn: Bool { bool(chatPreviews, default: true) }
    static var blurMediaOn: Bool { bool(blurMedia, default: false) }
    static var autoDownloadOn: Bool { bool(autoDownload, default: true) }
    static var protectIPOn: Bool { bool(protectIP, default: true) }
    static var cleanLinksOn: Bool { bool(cleanLinks, default: true) }

    static var currentTheme: AppTheme {
        AppTheme(rawValue: defaults.string(forKey: theme) ?? "") ?? .system
    }

    static var currentTextSize: TextSize {
        TextSize(rawValue: defaults.string(forKey: textSize) ?? "") ?? .medium
    }

    static var currentLockAfter: LockAfter {
        LockAfter(rawValue: defaults.integer(forKey: lockAfter)) ?? .never
    }
}

enum AppTheme: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }

    var label: String {
        switch self {
        case .system: return "System"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    /// Follow the system, or force light / dark for the whole app.
    func apply() {
        switch self {
        case .system: NSApp.appearance = nil
        case .light: NSApp.appearance = NSAppearance(named: .aqua)
        case .dark: NSApp.appearance = NSAppearance(named: .darkAqua)
        }
    }
}

enum TextSize: String, CaseIterable, Identifiable {
    case small, medium, large, larger
    var id: String { rawValue }

    var label: String {
        switch self {
        case .small: return "Small"
        case .medium: return "Medium"
        case .large: return "Large"
        case .larger: return "Larger"
        }
    }

    /// Multiplies the type scale (title 18 / body 14 / meta 12).
    var scale: CGFloat {
        switch self {
        case .small: return 0.9
        case .medium: return 1
        case .large: return 1.15
        case .larger: return 1.3
        }
    }
}

/// Lock Cursus (close the database) after this long without use.
enum LockAfter: Int, CaseIterable, Identifiable {
    case never = 0
    /// When Cursus is no longer the active app.
    case immediately = -1
    case oneMinute = 60
    case fiveMinutes = 300
    case fifteenMinutes = 900
    case oneHour = 3600
    var id: Int { rawValue }

    var label: String {
        switch self {
        case .never: return "Never"
        case .immediately: return "When I switch apps"
        case .oneMinute: return "After 1 minute"
        case .fiveMinutes: return "After 5 minutes"
        case .fifteenMinutes: return "After 15 minutes"
        case .oneHour: return "After 1 hour"
        }
    }
}

/// Locks Cursus after the chosen idle time (any key or mouse event counts
/// as use) or when it stops being the active app. AppKit calls back on the
/// main thread; `lock` is hopped onto the main actor explicitly.
final class IdleLock {
    private var lastActivity = Date()
    private var monitor: Any?
    private var timer: Timer?
    private var resign: NSObjectProtocol?
    private let lock: @MainActor () -> Void

    init(lock: @escaping @MainActor () -> Void) {
        self.lock = lock
    }

    func start() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyDown, .leftMouseDown, .rightMouseDown, .scrollWheel, .mouseMoved]
        ) { [weak self] event in
            self?.lastActivity = Date()
            return event
        }
        timer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            self?.check()
        }
        resign = NotificationCenter.default.addObserver(
            forName: NSApplication.didResignActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            if Prefs.currentLockAfter == .immediately { self?.fire() }
        }
    }

    private func check() {
        let after = Prefs.currentLockAfter.rawValue
        guard after > 0, Date().timeIntervalSince(lastActivity) >= TimeInterval(after) else { return }
        lastActivity = Date()
        fire()
    }

    private func fire() {
        let lock = self.lock
        Task { @MainActor in lock() }
    }
}
