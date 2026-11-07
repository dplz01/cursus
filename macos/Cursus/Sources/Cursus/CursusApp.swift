import AppKit
import CCursusBridge
import SwiftUI

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
            print("Cursus self-test OK — libsimplex from \(result.libsDir), profile “\(result.user)”, \(result.chats) chats")
            exit(0)
        } catch {
            FileHandle.standardError.write(Data("Cursus self-test FAILED: \(error.localizedDescription)\n".utf8))
            exit(1)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Built as a plain executable: make sure we get a Dock icon + menu
        // bar even when launched outside the bundle (`swift run`).
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
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
        WindowGroup {
            RootView()
                .environmentObject(model)
                .frame(minWidth: 760, minHeight: 500)
                // Native controls (switches, checkboxes, segmented pickers,
                // focus rings) in Cursus blue rather than the system accent.
                .accentColor(Theme.accent)
                .tint(Theme.accent)
        }
        // Title and toolbar on one line, the same height on every page.
        .windowToolbarStyle(.unified(showsTitle: true))
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
