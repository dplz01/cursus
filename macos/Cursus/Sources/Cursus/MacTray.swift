import AppKit

/// Shared by incoming calls and the menu-bar item; opening the app must not
/// change the conversation or expand an idle call panel.
@MainActor enum AppWindow {
    static func reveal() {
        NSApp.activate(ignoringOtherApps: true)
        if let window = NSApp.windows.first(where: { $0.canBecomeMain }) {
            window.makeKeyAndOrderFront(nil)
        } else if Notifier.available {
            NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL,
                configuration: NSWorkspace.OpenConfiguration(), completionHandler: nil)
        }
    }
}

/// AppKit also supports the older Intel Macs that predate MenuBarExtra.
@MainActor final class MacTray: NSObject, NSMenuDelegate {
    private weak var model: AppModel?
    private var item: NSStatusItem?
    private var observer: NSObjectProtocol?
    private let settings = NSMenuItem(title: "Settings…", action: #selector(openSettings), keyEquivalent: "")
    private let lock = NSMenuItem(title: "Lock Cursus", action: #selector(lockApp), keyEquivalent: "")

    init(model: AppModel) {
        self.model = model
        super.init()
        observer = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
        refresh()
    }

    deinit {
        if let observer = observer { NotificationCenter.default.removeObserver(observer) }
    }

    private func refresh() {
        guard Prefs.bool(Prefs.showTray, default: false) else {
            if let item = item { NSStatusBar.system.removeStatusItem(item) }
            item = nil
            return
        }
        guard item == nil else { return }
        let status = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        let image = NSImage(systemSymbolName: "bubble.left.and.bubble.right.fill", accessibilityDescription: "Cursus")
        image?.isTemplate = true
        status.button?.image = image
        status.button?.toolTip = "Cursus"
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        let open = NSMenuItem(title: "Open Cursus", action: #selector(openApp), keyEquivalent: "")
        let quit = NSMenuItem(title: "Quit Cursus", action: #selector(quitApp), keyEquivalent: "")
        for entry in [open, settings, lock] { entry.target = self; menu.addItem(entry) }
        menu.addItem(.separator())
        quit.target = self
        menu.addItem(quit)
        status.menu = menu
        item = status
    }

    func menuWillOpen(_ menu: NSMenu) {
        let ready = model?.phase == .ready
        settings.isEnabled = ready
        lock.isEnabled = ready
    }

    @objc private func openApp() { AppWindow.reveal() }
    @objc private func openSettings() {
        guard let model = model, model.phase == .ready else { return }
        model.destination = .settings
        AppWindow.reveal()
    }
    @objc private func lockApp() {
        guard let model = model, model.phase == .ready else { return }
        model.lock()
    }
    @objc private func quitApp() { NSApp.terminate(nil) }
}
