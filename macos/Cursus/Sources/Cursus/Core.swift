import Foundation
import CCursusBridge

/// Error text from the Rust core (already user-readable).
struct CoreError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// Decodes any JSON value and throws it away (methods that return null).
struct Nothing: Decodable {
    init(from decoder: Decoder) throws {}
    init() {}
}

private struct Envelope<T: Decodable>: Decodable {
    let ok: T?
    let error: String?
}

/// Calls into `cursus-core` through the C bridge. Every call blocks inside
/// libsimplex, so they run on a background queue; the core is thread-safe.
enum Core {
    private static let queue = DispatchQueue(
        label: "chat.cursus.core", qos: .userInitiated, attributes: .concurrent)

    /// Synchronous call — only for startup / self-test, never on the main
    /// thread while the UI is up.
    static func callSync<T: Decodable>(_ method: String, _ params: [String: Any] = [:], as type: T.Type) throws -> T {
        let request: [String: Any] = ["method": method, "params": params]
        let body = try JSONSerialization.data(withJSONObject: request)
        return try perform(body, as: type)
    }

    static func call<T: Decodable>(_ method: String, _ params: [String: Any] = [:], as type: T.Type) async throws -> T {
        let request: [String: Any] = ["method": method, "params": params]
        let body = try JSONSerialization.data(withJSONObject: request)
        return try await withCheckedThrowingContinuation { cont in
            queue.async {
                do {
                    cont.resume(returning: try perform(body, as: type))
                } catch {
                    cont.resume(throwing: error)
                }
            }
        }
    }

    /// Call a method whose result is not needed.
    static func run(_ method: String, _ params: [String: Any] = [:]) async throws {
        _ = try await call(method, params, as: Nothing.self)
    }

    private static func perform<T: Decodable>(_ body: Data, as type: T.Type) throws -> T {
        var bytes = [UInt8](body)
        bytes.append(0)
        let raw: UnsafeMutablePointer<CChar>? = bytes.withUnsafeBufferPointer { buf in
            buf.baseAddress!.withMemoryRebound(to: CChar.self, capacity: buf.count) { cursus_call($0) }
        }
        guard let raw = raw else { throw CoreError(message: "No reply from the chat core") }
        defer { cursus_free(raw) }
        let reply = Data(bytes: raw, count: strlen(raw))
        let envelope = try JSONDecoder().decode(Envelope<T>.self, from: reply)
        if let error = envelope.error { throw CoreError(message: error) }
        if let ok = envelope.ok { return ok }
        if T.self == Nothing.self, let nothing = Nothing() as? T { return nothing }
        throw CoreError(message: "Empty reply from the chat core")
    }
}

/// Pulls core events on a dedicated thread and hands them to the main actor.
final class EventPump {
    private let thread: Thread

    init(handler: @escaping @MainActor (CoreEvent) -> Void) {
        thread = Thread {
            let decoder = JSONDecoder()
            while true {
                guard let raw = cursus_next_event(500) else { continue }
                let data = Data(bytes: raw, count: strlen(raw))
                cursus_free(raw)
                guard let event = try? decoder.decode(CoreEvent.self, from: data) else { continue }
                Task { @MainActor in handler(event) }
            }
        }
        thread.name = "cursus-events"
        thread.qualityOfService = .userInitiated
    }

    func start() {
        thread.start()
    }
}
