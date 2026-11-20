import AppKit
import AVFoundation
import CCursusBridge
import SwiftUI
import UserNotifications

@main
enum Launcher {
    static func main() {
        // Before any other thread: the core sets TMPDIR for libsimplex.
        cursus_init()
        if CommandLine.arguments.contains("--self-test") {
            SelfTest.run()
        }
        CursusApp.main()
    }
}

/// `Cursus.app/Contents/MacOS/Cursus --self-test`: load libsimplex, open a
/// throwaway database, print the result. Proves the core runs on this Mac.
enum SelfTest {
    static func run() -> Never {
        var params: [String: Any] = [:]
        if let dir = Bundle.main.privateFrameworksPath,
           FileManager.default.fileExists(atPath: dir + "/libsimplex.dylib") {
            params["libsDir"] = dir
        }
        do {
            let result = try Core.callSync("selfTest", params, as: SelfTestResult.self)
            // Exercise the C PCM ABI without opening the microphone on CI.
            var pcm = [Int16](repeating: 123, count: 960)
            let status = pcm.withUnsafeMutableBufferPointer { cursus_call_playback($0.baseAddress, 960) }
            guard status == 0, pcm.allSatisfy({ $0 == 0 }) else { throw CoreError(message: "Idle PCM playback must be silent") }
            var frames = [[Int16]]()
            let capture = CallCaptureFrames { frames.append(Array($0)) }
            let samples = [Float](repeating: 0.25, count: 1920)
            samples.withUnsafeBufferPointer { capture.append($0) }
            guard frames.count == 2, frames.allSatisfy({ $0.count == 960 && $0.allSatisfy({ $0 == 8192 }) }) else {
                throw CoreError(message: "PCM capture framing failed")
            }
            // Validate the real microphone resampler for native mono and
            // 44.1 kHz stereo devices; no input device or permission needed.
            for rate in [48_000.0, 44_100.0] {
                guard let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2),
                      let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4410) else {
                    throw CoreError(message: "Cannot allocate capture self-test buffer")
                }
                buffer.frameLength = 4410
                for channel in 0..<2 {
                    for i in 0..<4410 { buffer.floatChannelData![channel][i] = 0.25 }
                }
                var converted = [[Int16]]()
                var failure: String?
                let converter = try CallInputConverter(format: format,
                    send: { converted.append(Array($0)) }, failed: { failure = $0 })
                converter.append(buffer)
                guard failure == nil, converter.hasInput, !converted.isEmpty,
                      converted.allSatisfy({ $0.count == 960 && $0.contains(where: { abs(Int($0)) > 100 }) }) else {
                    throw CoreError(message: failure ?? "Microphone conversion produced no PCM at \(rate) Hz")
                }
            }
            print("Cursus self-test OK — libsimplex from \(result.libsDir), profile “\(result.user)”, \(result.chats) chats")
            exit(0)
        } catch {
            FileHandle.standardError.write(Data("Cursus self-test FAILED: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    private var tray: MacTray?

    @MainActor func configureTray(model: AppModel) {
        if tray == nil { tray = MacTray(model: model) }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Built as a plain executable: make sure we get a Dock icon + menu
        // bar even when launched outside the bundle (`swift run`).
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        Prefs.watchDownloadPolicy()
        if Notifier.available {
            UNUserNotificationCenter.current().delegate = self
            Task { @MainActor in MacCallNotifications.configure() }
        }
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                                withCompletionHandler completion: @escaping (UNNotificationPresentationOptions) -> Void) {
        DispatchQueue.main.async {
            let visible = NSApp.isActive && NSApp.windows.contains { $0.isVisible && $0.canBecomeMain }
            completion(notification.request.content.categoryIdentifier == "cursus-call" && !visible ? [.banner, .sound] : [])
        }
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                                withCompletionHandler completion: @escaping () -> Void) {
        let content = response.notification.request.content
        if content.categoryIdentifier == "cursus-call", let id = content.userInfo["contactId"] as? NSNumber,
           response.actionIdentifier != UNNotificationDismissActionIdentifier {
            let action = response.actionIdentifier == "answer" ? "answer" : response.actionIdentifier == "decline" ? "decline" : "open"
            DispatchQueue.main.async {
                NotificationCenter.default.post(name: .cursusCallAction, object: nil,
                                                userInfo: ["action": action, "contactId": id.int64Value])
            }
        }
        completion()
    }

    /// Keep running with the window closed so messages keep arriving; the
    /// Dock icon reopens it.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Close the database cleanly (synchronous: the process ends next).
        _ = try? Core.callSync("close", as: Nothing.self)
    }
}

struct CursusApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        // Native traffic lights sit inside the floating sidebar surface.
        WindowGroup("Cursus") {
            RootView()
                .environmentObject(model)
                .onAppear { delegate.configureTray(model: model) }
                .frame(minWidth: 760, minHeight: 500)
                // Native controls (switches, checkboxes, segmented pickers,
                // focus rings) in Cursus blue rather than the system accent.
                .accentColor(Theme.accent)
                .tint(Theme.accent)
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Chat…") { model.showNewChat = true }
                    .keyboardShortcut("n")
                    .disabled(model.phase != .ready)
            }
            CommandGroup(replacing: .appSettings) {
                Button("Settings…") {
                    model.destination = .settings
                }
                .keyboardShortcut(",")
                .disabled(model.phase != .ready)
            }
            CommandMenu("Chat") {
                Button(model.showInfo ? "Hide Info" : "Show Info") { model.toggleInfo() }
                    .keyboardShortcut("i")
                    .disabled(model.phase != .ready || model.selectedChat == nil
                              || model.selectedChat?.isNotes == true)
                Divider()
                Button("Next Conversation") { model.selectAdjacent(1) }
                    .keyboardShortcut(.tab, modifiers: .control)
                    .disabled(model.phase != .ready)
                Button("Previous Conversation") { model.selectAdjacent(-1) }
                    .keyboardShortcut(.tab, modifiers: [.control, .shift])
                    .disabled(model.phase != .ready)
            }
            CommandMenu("Account") {
                Button("Lock Cursus") { model.lock() }
                    .keyboardShortcut("l", modifiers: [.command, .shift])
                    .disabled(model.phase != .ready)
            }
        }
    }
}
