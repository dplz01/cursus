import AppKit
import AVFoundation
import Foundation
import SwiftUI
import UserNotifications

struct NativeCall: Decodable, Equatable {
    let contactId: Int64
    let name: String
    let phase: String
    let outgoing: Bool
    let muted: Bool
    let encrypted: Bool
    let durationSecs: Int
    var label: String {
        switch phase {
        case "incoming": return "Incoming audio call"
        case "ringing": return "Calling…"
        case "connected": return Format.duration(durationSecs)
        default: return "Connecting…"
        }
    }
}
struct NativeCallState: Decodable, Equatable {
    let active: NativeCall?
    let error: String?
    static let empty = NativeCallState(active: nil, error: nil)
}
struct CallRecord: Decodable, Identifiable {
    let itemId: Int64
    let contactId: Int64
    let peerName: String
    let outgoing: Bool
    let status: String
    let durationSecs: Int
    let ts: String?
    var id: Int64 { itemId }
    var missed: Bool { !outgoing && ["missed", "pending"].contains(status) }
    var label: String {
        if missed { return "Missed" }
        if status == "rejected" { return "Declined" }
        if status == "error" { return "Failed" }
        return outgoing ? "Outgoing" : "Incoming"
    }
}

@MainActor
final class MacCalls: ObservableObject {
    @Published private(set) var state = NativeCallState.empty
    @Published private(set) var busy = false
    @Published var expanded = false
    @Published private(set) var history: [CallRecord] = []
    @Published private(set) var loadingHistory = false
    private weak var model: AppModel?
    private let audio = MacCallAudio()
    private var monitor: Task<Void, Never>?
    private var epoch = 0
    private var lastError: String?
    private var historyStale = true
    private var ringtone: NSSound?
    private var activity: NSObjectProtocol?
    private var notificationObserver: NSObjectProtocol?

    func startMonitoring(model: AppModel) {
        self.model = model
        guard monitor == nil else { return }
        notificationObserver = NotificationCenter.default.addObserver(forName: .cursusCallAction, object: nil, queue: .main) { [weak self] note in
            let action = note.userInfo?["action"] as? String ?? "open"
            let id = note.userInfo?["contactId"] as? Int64
            Task { @MainActor [weak self] in
                guard let self = self, let id = id, self.state.active?.contactId == id else { return }
                if action == "decline" { self.end(reject: true) }
                else { self.reveal(); if action == "answer" { self.accept() } }
            }
        }
        monitor = Task { [weak self] in
            while !Task.isCancelled {
                guard let self = self else { return }
                if self.model?.phase == .ready && !self.busy {
                    let epoch = self.epoch
                    do {
                        let state = try await Core.call("callState", as: NativeCallState.self)
                        if self.epoch == epoch && !self.busy && self.model?.phase == .ready { self.update(state) }
                    } catch { /* A closing database is retried after unlock. */ }
                }
                try? await Task.sleep(nanoseconds: 400_000_000)
            }
        }
    }

    func start(_ contact: Chat) {
        guard !busy, state.active == nil, contact.isDirect, model?.phase == .ready else { return }
        perform("callStart", ["contactId": contact.apiId, "name": contact.title], permission: true)
    }
    func accept() {
        guard !busy, state.active?.phase == "incoming" else { return }
        perform("callAccept", permission: true)
    }
    func mute() {
        guard !busy, let call = state.active else { return }
        perform("callMute", ["muted": !call.muted])
    }
    func end(reject: Bool = false) {
        audio.stop() // Stop the microphone before waiting for signaling.
        perform(reject ? "callReject" : "callEnd", allowBusy: true)
    }
    private func perform(_ method: String, _ params: [String: Any] = [:], permission: Bool = false, allowBusy: Bool = false) {
        guard !busy || allowBusy else { return }
        epoch += 1
        let ticket = epoch
        busy = true
        if permission { expanded = true }
        Task {
            defer { if epoch == ticket { busy = false } }
            do {
                if permission {
                    let allowed: Bool
                    switch AVCaptureDevice.authorizationStatus(for: .audio) {
                    case .authorized: allowed = true
                    case .notDetermined:
                        allowed = await withCheckedContinuation { continuation in
                            AVCaptureDevice.requestAccess(for: .audio) { continuation.resume(returning: $0) }
                        }
                    default: allowed = false
                    }
                    guard epoch == ticket, model?.phase == .ready else { return }
                    guard allowed else { throw CoreError(message: "Allow microphone access in macOS privacy settings to make calls.") }
                    model?.cancelVoice()
                    VoicePlayer.shared.stop()
                }
                let state = try await Core.call(method, params, as: NativeCallState.self)
                if epoch == ticket { update(state) }
            } catch {
                if epoch == ticket { model?.report(error); if state.active == nil { expanded = false } }
            }
        }
    }
    func reveal() {
        expanded = true
        AppWindow.reveal()
    }
    /// Closing the account invalidates in-flight permission/command results.
    /// The bridge closes the remote call as part of `close` / `wipeLocalData`.
    func stopForLock() {
        epoch += 1; busy = false
        update(.empty)
        history = []; historyStale = true
    }
    private func update(_ next: NativeCallState) {
        let previous = state.active
        if state != next { state = next }
        if VoicePlayer.shared.callActive != (next.active != nil) { VoicePlayer.shared.callActive = next.active != nil }
        if let error = next.error, error != lastError { model?.errorMessage = error }
        lastError = next.error
        guard let call = next.active else {
            audio.stop(); ringtone?.stop(); ringtone = nil
            MacCallNotifications.clear()
            if let activity = activity { ProcessInfo.processInfo.endActivity(activity); self.activity = nil }
            expanded = false
            if previous != nil { historyStale = true; Task { await loadHistory() } }
            return
        }
        if activity == nil {
            activity = ProcessInfo.processInfo.beginActivity(options: [.userInitiated, .idleSystemSleepDisabled], reason: "Cursus audio call")
        }
        if previous?.contactId != call.contactId { VoicePlayer.shared.stop(); expanded = true }
        if call.phase == "incoming" {
            if NSApp.isActive && NSApp.windows.contains(where: { $0.isVisible && $0.canBecomeMain }) {
                MacCallNotifications.clear()
                if ringtone == nil { ringtone = NSSound(named: NSSound.Name("Glass")); ringtone?.loops = true }
                if ringtone?.isPlaying == false { ringtone?.play() }
            } else {
                ringtone?.stop(); ringtone = nil
                MacCallNotifications.post(call)
            }
        } else {
            ringtone?.stop(); ringtone = nil; MacCallNotifications.clear()
            if ["connecting", "connected"].contains(call.phase) {
                let ticket = epoch
                audio.start { [weak self] error in
                    Task { @MainActor in
                        guard let self = self, self.epoch == ticket, self.state.active?.contactId == call.contactId else { return }
                        self.model?.errorMessage = error
                        self.end()
                    }
                }
            }
        }
    }
    func loadHistory() async {
        guard !loadingHistory, historyStale, model?.phase == .ready else { return }
        loadingHistory = true
        let ticket = epoch
        defer { loadingHistory = false }
        do {
            let entries = try await Core.call("callHistory", as: [CallRecord].self)
            guard model?.phase == .ready else { return }
            guard ticket == epoch else { Task { await loadHistory() }; return }
            history = entries; historyStale = false
        } catch { if model?.phase == .ready && ticket == epoch { model?.report(error) } }
    }
}

extension Notification.Name { static let cursusCallAction = Notification.Name("chat.cursus.call-action") }

@MainActor
enum MacCallNotifications {
    private static var contact: Int64?
    static func configure() {
        guard Notifier.available else { return }
        let answer = UNNotificationAction(identifier: "answer", title: "Answer", options: [.foreground])
        let decline = UNNotificationAction(identifier: "decline", title: "Decline", options: [.destructive])
        let category = UNNotificationCategory(identifier: "cursus-call", actions: [answer, decline], intentIdentifiers: [], options: [])
        UNUserNotificationCenter.current().setNotificationCategories([category])
    }
    static func post(_ call: NativeCall) {
        guard Notifier.available, contact != call.contactId else { return }
        contact = call.contactId
        let content = UNMutableNotificationContent()
        // Match the app's default notification privacy policy.
        let hide = UserDefaults.standard.object(forKey: AppModel.notificationsHideTextKey) as? Bool ?? true
        content.title = hide ? "Cursus" : call.name
        content.body = "Incoming audio call"
        content.sound = .default
        content.categoryIdentifier = "cursus-call"
        content.userInfo = ["contactId": call.contactId]
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "cursus-incoming-call", content: content, trigger: nil))
    }
    static func clear() {
        guard contact != nil, Notifier.available else { return }
        contact = nil
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["cursus-incoming-call"])
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: ["cursus-incoming-call"])
    }
}
