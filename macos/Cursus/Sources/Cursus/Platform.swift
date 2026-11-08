import AppKit
import CoreImage.CIFilterBuiltins
import Security
import SwiftUI
import UserNotifications

/// The database password in the login Keychain ("Remember password").
enum Keychain {
    private static let service = "chat.cursus.Cursus"
    private static let account = "database"

    private static var query: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    static func load() -> String? {
        var q = query
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
              let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func save(_ password: String) -> Bool {
        delete()
        var q = query
        q[kSecValueData as String] = Data(password.utf8)
        q[kSecAttrLabel as String] = "Cursus database password"
        return SecItemAdd(q as CFDictionary, nil) == errSecSuccess
    }

    static func delete() {
        SecItemDelete(query as CFDictionary)
    }

    static var hasPassword: Bool { load() != nil }
}

/// Design tokens — the values of `Tokens::light()` / `Tokens::dark()` in
/// linux/src/theme.rs (see docs/DESIGN_SYSTEM.md), so both apps look alike.
enum Theme {
    private static func dynamic(_ light: UInt32, _ dark: UInt32, alpha: CGFloat = 1, darkAlpha: CGFloat? = nil) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: isDark ? (darkAlpha ?? alpha) : alpha)
        })
    }

    // Surfaces
    static let app = dynamic(0xEEF0F4, 0x0E1015)
    static let sidebar = dynamic(0xFFFFFF, 0x15181E)
    static let list = dynamic(0xFFFFFF, 0x15181E)
    static let panel = dynamic(0xEEF0F4, 0x0E1015)
    static let elevated = dynamic(0xFFFFFF, 0x1C2028)
    static let hover = dynamic(0xEDEFF3, 0x20242D)
    static let fieldFill = dynamic(0xF2F4F7, 0x111318)
    static let border = dynamic(0xDDE1E7, 0x252A33)

    // Text
    static let primaryText = dynamic(0x12141A, 0xF2F3F5)
    static let secondaryText = dynamic(0x5C6370, 0xA0A6B3)
    static let mutedText = dynamic(0x8B919C, 0x6E7585)

    // Accent
    static let accent = dynamic(0x2F6FED, 0x5B8CFF)
    static let accentHover = dynamic(0x255DD1, 0x6B99FF)
    static let accentPress = dynamic(0x1C4CB8, 0x4D7AEB)
    static let accentOn = dynamic(0xFFFFFF, 0x0B1020)
    static let accentSoft = dynamic(0x2F6FED, 0x5B8CFF, alpha: 0.12, darkAlpha: 0.16)
    static let accentSoftHover = dynamic(0x2F6FED, 0x5B8CFF, alpha: 0.20, darkAlpha: 0.26)
    static let accentSoftPress = dynamic(0x2F6FED, 0x5B8CFF, alpha: 0.30, darkAlpha: 0.36)
    static let disabledFill = dynamic(0xD1D5DA, 0x404D66)

    // Status
    static let danger = dynamic(0xC62828, 0xFF6B6B)
    static let dangerHover = dynamic(0xB01C1C, 0xD94747)
    static let dangerPress = dynamic(0x941414, 0xB33333)
    static let dangerSoft = dynamic(0xC62828, 0xFF6B6B, alpha: 0.14)
    static let dangerSoftPress = dynamic(0xC62828, 0xFF6B6B, alpha: 0.24)
    static let success = dynamic(0x2E7D32, 0x6BCB77)
    static let gold = dynamic(0xB58B0F, 0xECC14F)

    // Bubbles
    static let bubbleIn = dynamic(0xFFFFFF, 0x1C2028)
    static let bubbleOut = dynamic(0xD8E5FF, 0x243A63)
    static let bubbleOutMeta = dynamic(0x5A6C8F, 0xA7B6D6)
    static let bubbleOutTick = dynamic(0x2F6FED, 0x9CBBFF)

    static func formatColor(_ name: String?) -> Color? {
        switch name {
        case "red": return .red
        case "green": return .green
        case "blue": return .blue
        case "yellow": return .yellow
        case "cyan": return .cyan
        case "magenta": return .pink
        case "black": return .primary
        case "white": return .secondary
        default: return nil
        }
    }
}

/// Shapes (DESIGN_SYSTEM.md → Spacing, radius and shape). Use these, never
/// inline numbers.
enum Radius {
    static let sm: CGFloat = 8
    static let md: CGFloat = 10
    static let lg: CGFloat = 12
    static let bubble: CGFloat = 16
}

/// Type scale (desktop): title 18 / body 14 / meta 12, times the text size
/// chosen in Settings → Appearance (RootView rebuilds when it changes).
extension Font {
    private static var scale: CGFloat { Prefs.currentTextSize.scale }

    static var cTitle: Font { .system(size: 18 * scale, weight: .semibold) }
    static var cBody: Font { .system(size: 14 * scale) }
    static var cBodyBold: Font { .system(size: 14 * scale, weight: .semibold) }
    static var cMeta: Font { .system(size: 12 * scale) }
    static var cMetaBold: Font { .system(size: 12 * scale, weight: .semibold) }
}

/// Cursus's own outlined icons (linux/assets/icons, converted to vector PDF
/// templates in macos/Resources/icons and copied into the bundle by
/// build.sh). Outside the bundle (`swift run`) an SF Symbol stands in.
enum Icons {
    private static var cache: [String: NSImage] = [:]

    static func image(_ name: String) -> NSImage? {
        if name.hasPrefix("sf:") { return nil }
        if let hit = cache[name] { return hit }
        guard let url = Bundle.main.url(forResource: name, withExtension: "pdf", subdirectory: "icons"),
              let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        cache[name] = image
        return image
    }

    /// SF Symbol for a Cursus icon name; `sf:<symbol>` asks for one directly
    /// (glyphs Cursus has no drawing for yet).
    static func fallback(_ name: String) -> String {
        if name.hasPrefix("sf:") { return String(name.dropFirst(3)) }
        switch name {
        case "chats": return "bubble.left"
        case "channels": return "antenna.radiowaves.left.and.right"
        case "calls": return "phone"
        case "settings": return "gearshape"
        case "send", "paperplane": return "paperplane"
        case "star": return "star"
        case "plus": return "plus"
        case "copy": return "doc.on.doc"
        case "paste": return "doc.on.clipboard"
        case "trash-delete": return "trash"
        case "session-leave": return "rectangle.portrait.and.arrow.right"
        case "clear-all": return "eraser"
        case "mute-notification": return "bell.slash"
        case "preferences": return "slider.horizontal.3"
        case "back": return "chevron.left"
        case "chevron-down": return "chevron.down"
        case "info-circle": return "info.circle"
        case "lock": return "lock"
        case "reply": return "arrowshape.turn.up.left"
        case "edit-3": return "pencil"
        case "download": return "arrow.down.circle"
        case "check": return "checkmark"
        case "check-read": return "checkmark.circle"
        case "system-pending-line": return "clock"
        case "user-group": return "person.2"
        case "user-alt-1": return "person"
        case "user-plus-alt-1": return "person.badge.plus"
        case "link-minimalistic-2": return "link"
        case "address-card": return "person.text.rectangle"
        case "shield-check": return "checkmark.shield"
        case "image-pen", "image-square": return "photo"
        case "owner": return "crown"
        case "admin", "admin-role": return "person.badge.key"
        case "user-member": return "person"
        case "eye-observer": return "eye"
        case "cancel": return "xmark"
        case "mic": return "mic"
        case "play": return "play.fill"
        case "stop": return "pause.fill"
        case "timer": return "timer"
        case "history": return "clock.arrow.circlepath"
        case "review-file": return "doc.text"
        default: return "questionmark"
        }
    }
}

/// One tinted Cursus glyph. Without `color` it takes the surrounding
/// foreground colour (so button styles can tint it on hover).
struct CIcon: View {
    let name: String
    var size: CGFloat = 20
    var color: Color? = nil

    init(_ name: String, size: CGFloat = 20, color: Color? = nil) {
        self.name = name
        self.size = size
        self.color = color
    }

    var body: some View {
        glyph
            .frame(width: size, height: size)
            .foregroundColor(color)
    }

    @ViewBuilder private var glyph: some View {
        if let image = Icons.image(name) {
            Image(nsImage: image)
                .renderingMode(.template)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
        } else {
            Image(systemName: Icons.fallback(name))
                .resizable()
                .aspectRatio(contentMode: .fit)
                .padding(size * 0.06)
        }
    }
}

enum Images {
    /// `data:image/…;base64,…` (profile pictures) → image.
    private static let cache = NSCache<NSString, NSImage>()

    static func fromDataURL(_ url: String?) -> NSImage? {
        guard let url = url, let comma = url.range(of: "base64,") else { return nil }
        if let hit = cache.object(forKey: url as NSString) { return hit }
        guard let data = Data(base64Encoded: String(url[comma.upperBound...]), options: .ignoreUnknownCharacters),
              let image = NSImage(data: data)
        else { return nil }
        cache.setObject(image, forKey: url as NSString)
        return image
    }

    /// Square JPEG data URL small enough for a SimpleX profile (≤ 12 500 chars).
    static func profileDataURL(from image: NSImage) -> String? {
        for side in [192, 160, 128, 96, 64] {
            guard let square = squareCrop(image, side: side) else { return nil }
            for quality in [0.8, 0.6, 0.45, 0.3] {
                guard let tiff = square.tiffRepresentation,
                      let rep = NSBitmapImageRep(data: tiff),
                      let jpeg = rep.representation(using: .jpeg, properties: [.compressionFactor: quality])
                else { continue }
                let url = "data:image/jpg;base64," + jpeg.base64EncodedString()
                if url.count <= 12_500 { return url }
            }
        }
        return nil
    }

    private static func squareCrop(_ image: NSImage, side: Int) -> NSImage? {
        guard let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else { return nil }
        let edge = min(cg.width, cg.height)
        let crop = CGRect(x: (cg.width - edge) / 2, y: (cg.height - edge) / 2, width: edge, height: edge)
        guard let cropped = cg.cropping(to: crop) else { return nil }
        let out = NSImage(size: NSSize(width: side, height: side))
        out.lockFocus()
        NSGraphicsContext.current?.imageInterpolation = .high
        NSImage(cgImage: cropped, size: .zero)
            .draw(in: NSRect(x: 0, y: 0, width: side, height: side))
        out.unlockFocus()
        return out
    }

    static func qrCode(_ text: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 8, y: 8)),
              let cg = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return NSImage(cgImage: cg, size: NSSize(width: output.extent.width / 2, height: output.extent.height / 2))
    }
}

enum Clipboard {
    static func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

/// macOS notifications; silently off when not running from an app bundle
/// (`swift run`), where UNUserNotificationCenter would crash.
enum Notifier {
    static var available: Bool { Bundle.main.bundleIdentifier != nil && Bundle.main.bundlePath.hasSuffix(".app") }

    static func requestAuthorization() {
        guard available else { return }
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    static func post(title: String, body: String, chat: String) {
        guard available else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        content.threadIdentifier = chat
        content.userInfo = ["chat": chat]
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    static func clear(chat: String) {
        guard available else { return }
        let center = UNUserNotificationCenter.current()
        center.getDeliveredNotifications { delivered in
            let ids = delivered
                .filter { ($0.request.content.userInfo["chat"] as? String) == chat }
                .map { $0.request.identifier }
            center.removeDeliveredNotifications(withIdentifiers: ids)
        }
    }
}

enum Format {
    static func bytes(_ n: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: n, countStyle: .file)
    }

    static func duration(_ seconds: Int) -> String {
        String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}


// MARK: - Window chrome

/// Configures the hosting NSWindow: a full-width title strip ("Cursus" +
/// traffic lights, like Telegram) with every column below it and no system
/// separator line — each pane draws its own Cursus header right below it.
/// Applied whenever the view joins a window (also a window reopened from
/// the Dock after closing the last one).
struct WindowChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { ChromeView() }
    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? ChromeView)?.apply()
    }

    final class ChromeView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            apply()
        }

        func apply() {
            guard let window = window else { return }
            window.title = "Cursus"
            window.titleVisibility = .visible
            // The split view's sidebar otherwise runs up under the title bar
            // and the strip covers only the detail pane.
            if window.styleMask.contains(.fullSizeContentView) {
                window.styleMask.remove(.fullSizeContentView)
            }
            window.titlebarAppearsTransparent = false
            window.titlebarSeparatorStyle = .none
        }
    }
}
