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

/// Design tokens (docs/DESIGN_SYSTEM.md), light / dark.
enum Theme {
    private static func dynamic(_ light: UInt32, _ dark: UInt32, alpha: CGFloat = 1) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            let hex = isDark ? dark : light
            return NSColor(
                srgbRed: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: alpha)
        })
    }

    static let accent = dynamic(0x2F6FED, 0x5B8CFF)
    static let accentSoft = dynamic(0x2F6FED, 0x5B8CFF, alpha: 0.17)
    static let panel = dynamic(0xFFFFFF, 0x1C2028)
    static let elevated = dynamic(0xF0F2F5, 0x262B36)
    static let secondaryText = dynamic(0x5C6370, 0xA0A6B3)
    static let mutedText = dynamic(0x8B919C, 0x6E7585)
    static let fieldFill = dynamic(0xEBEDF0, 0x181B22)

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
