import AppKit
import AVFoundation
import AVKit
import SwiftUI

// Voice recording, waveforms, video posters and the media gallery — native
// AVFoundation / AVKit (Linux: ffmpeg + mpv + its own gallery).

// MARK: - Waveform

/// Peak bars of an audio file, 0…1 (loudest = 1), for voice messages.
enum Waveform {
    private static var cache: [String: [Float]] = [:]
    private static let lock = NSLock()

    static func cached(_ url: URL) -> [Float]? {
        lock.lock()
        defer { lock.unlock() }
        return cache[url.path]
    }

    /// Reads the file (off the main thread); nil when it can't be decoded.
    static func peaks(_ url: URL, bars: Int = 48) -> [Float]? {
        if let hit = cached(url) { return hit }
        guard let file = try? AVAudioFile(forReading: url) else { return nil }
        let frames = AVAudioFrameCount(file.length)
        guard frames > 0,
              let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: frames),
              (try? file.read(into: buffer)) != nil,
              let samples = buffer.floatChannelData?[0] else { return nil }
        let peaks = Waveform.bars(UnsafeBufferPointer(start: samples, count: Int(buffer.frameLength)), bars: bars)
        lock.lock()
        cache[url.path] = peaks
        lock.unlock()
        return peaks
    }

    static func bars(_ samples: UnsafeBufferPointer<Float>, bars: Int) -> [Float] {
        let n = samples.count
        guard n > 0, bars > 0 else { return Array(repeating: 0.1, count: max(bars, 1)) }
        let per = max(1, n / bars)
        var out: [Float] = []
        out.reserveCapacity(bars)
        for b in 0..<bars {
            let start = b * per
            guard start < n else {
                out.append(0)
                continue
            }
            var peak: Float = 0
            for i in start..<min(n, start + per) { peak = max(peak, abs(samples[i])) }
            out.append(peak)
        }
        return normalized(out)
    }

    /// Loudest bar = 1; a floor keeps silent parts visible.
    static func normalized(_ values: [Float]) -> [Float] {
        let top = values.max() ?? 0
        guard top > 0 else { return values.map { _ in 0.08 } }
        return values.map { max(0.08, $0 / top) }
    }
}

/// Bars, the played part in `played`, the rest in `rest`. Split into small
/// typed helpers: one big expression was too much for Swift 5.7's checker.
struct WaveformView: View {
    let peaks: [Float]
    var progress: Double = 0
    var played: Color = Theme.accent
    var rest: Color = Theme.mutedText

    private static let gap: CGFloat = 2

    var body: some View {
        GeometryReader { geo in
            bars(in: geo.size)
        }
    }

    private func bars(in size: CGSize) -> some View {
        let count = max(peaks.count, 1)
        let width = barWidth(total: size.width, count: count)
        return HStack(alignment: .center, spacing: Self.gap) {
            ForEach(0..<peaks.count, id: \.self) { index in
                bar(index: index, count: count, width: width, height: size.height)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .leading)
    }

    private func barWidth(total: CGFloat, count: Int) -> CGFloat {
        let gaps: CGFloat = Self.gap * CGFloat(count - 1)
        let each: CGFloat = (total - gaps) / CGFloat(count)
        return max(1.5, each)
    }

    private func bar(index: Int, count: Int, width: CGFloat, height: CGFloat) -> some View {
        let isPlayed: Bool = Double(index) / Double(count) < progress
        let level: CGFloat = CGFloat(peaks[index])
        let barHeight: CGFloat = max(3, height * level)
        return Capsule()
            .fill(isPlayed ? played : rest)
            .frame(width: width, height: barHeight)
    }
}

// MARK: - Recording

/// A finished take, ready to send or discard.
struct VoiceDraft: Equatable {
    let url: URL
    let duration: Int
    let peaks: [Float]
}

/// Records AAC m4a (16 kHz mono 32 kbit/s — what official clients send),
/// with live levels for the recording bar.
final class VoiceRecorder: NSObject, ObservableObject {
    @Published private(set) var recording = false
    @Published private(set) var elapsed: TimeInterval = 0
    /// Recent levels 0…1, newest last.
    @Published private(set) var levels: [Float] = []

    private var recorder: AVAudioRecorder?
    private var timer: Timer?
    private var url: URL?
    /// Every level since the start, for the draft's waveform.
    private var history: [Float] = []

    enum RecorderError: LocalizedError {
        case denied, failed
        var errorDescription: String? {
            switch self {
            case .denied:
                return "Cursus can't use the microphone. Allow it in System Settings → Privacy & Security → Microphone."
            case .failed:
                return "The microphone couldn't start recording."
            }
        }
    }

    /// Asks for the microphone the first time.
    static func allowed() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: return true
        case .notDetermined:
            return await withCheckedContinuation { cont in
                AVCaptureDevice.requestAccess(for: .audio) { cont.resume(returning: $0) }
            }
        default: return false
        }
    }

    /// Main actor: the level timer needs the main run loop (and @Published).
    @MainActor
    func start() async throws {
        guard !recording else { return }
        guard await Self.allowed() else { throw RecorderError.denied }
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("voice-\(Int(Date().timeIntervalSince1970 * 1000)).m4a")
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
            AVSampleRateKey: 16_000,
            AVNumberOfChannelsKey: 1,
            AVEncoderBitRateKey: 32_000,
        ]
        guard let recorder = try? AVAudioRecorder(url: url, settings: settings) else { throw RecorderError.failed }
        recorder.isMeteringEnabled = true
        guard recorder.record() else { throw RecorderError.failed }
        self.recorder = recorder
        self.url = url
        history = []
        levels = []
        elapsed = 0
        recording = true
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in self?.tick() }
    }

    private func tick() {
        guard let recorder = recorder else { return }
        recorder.updateMeters()
        // dB (−160…0) → 0…1, with a 50 dB window.
        let level = max(0, min(1, (recorder.averagePower(forChannel: 0) + 50) / 50))
        history.append(level)
        levels = Array((levels + [level]).suffix(40))
        elapsed = recorder.currentTime
    }

    /// Stop and keep the take (nil when too short to send).
    func stop() -> VoiceDraft? {
        guard let recorder = recorder, let url = url else { return nil }
        let duration = recorder.currentTime
        finish()
        guard duration >= 0.5 else {
            try? FileManager.default.removeItem(at: url)
            return nil
        }
        return VoiceDraft(url: url, duration: max(1, Int(duration.rounded())), peaks: Self.resample(history, to: 48))
    }

    func cancel() {
        let url = self.url
        finish()
        if let url = url { try? FileManager.default.removeItem(at: url) }
    }

    private func finish() {
        timer?.invalidate()
        timer = nil
        recorder?.stop()
        recorder = nil
        url = nil
        recording = false
        levels = []
    }

    private static func resample(_ values: [Float], to count: Int) -> [Float] {
        guard !values.isEmpty else { return Array(repeating: 0.08, count: count) }
        let out = (0..<count).map { i -> Float in
            let start = i * values.count / count
            let end = max(start + 1, (i + 1) * values.count / count)
            return values[start..<min(end, values.count)].max() ?? 0
        }
        return Waveform.normalized(out)
    }
}

/// Replaces the composer while recording: ✕, red dot, time, live levels, ■.
struct RecordingBar: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var recorder: VoiceRecorder

    private var liveLevels: [Float] {
        let levels: [Float] = recorder.levels
        if levels.isEmpty { return [0.08] }
        return levels.map { (level: Float) -> Float in max(0.08, level) }
    }

    var body: some View {
        HStack(spacing: 10) {
            IconTool(icon: "cancel", help: "Discard (Esc)", size: 16) { model.cancelVoice() }
                .keyboardShortcut(.cancelAction)
            Circle().fill(Theme.danger).frame(width: 8, height: 8)
            Text(Format.duration(Int(recorder.elapsed)))
                .font(.cMeta.monospacedDigit())
                .foregroundColor(Theme.primaryText)
            WaveformView(peaks: liveLevels, progress: 1, played: Theme.danger.opacity(0.7))
                .frame(height: 26)
            Button { model.stopRecording() } label: {
                CIcon("stop", size: 16, color: .white)
                    .frame(width: 34, height: 34)
                    .background(Circle().fill(Theme.danger))
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.defaultAction)
            .help("Stop (Return)")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Theme.list)
    }
}

/// A finished take: ✕, play / pause, waveform, length, send.
struct VoiceDraftBar: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var player = VoicePlayer.shared
    let draft: VoiceDraft

    private static let previewId: Int64 = -1

    var body: some View {
        HStack(spacing: 10) {
            IconTool(icon: "cancel", help: "Discard (Esc)", size: 16) {
                player.stop()
                model.cancelVoice()
            }
            .keyboardShortcut(.cancelAction)
            Button { player.toggle(id: Self.previewId, url: draft.url) } label: {
                CIcon(player.playingId == Self.previewId ? "stop" : "play", size: 14, color: Theme.accentOn)
                    .frame(width: 30, height: 30)
                    .background(Circle().fill(Theme.accent))
            }
            .buttonStyle(.plain)
            .help("Listen")
            WaveformView(peaks: draft.peaks,
                         progress: player.playingId == Self.previewId ? player.progress : 0)
                .frame(height: 26)
            Text(Format.duration(draft.duration))
                .font(.cMeta.monospacedDigit())
                .foregroundColor(Theme.secondaryText)
            Button {
                player.stop()
                Task { await model.sendVoiceDraft() }
            } label: {
                CIcon("send", size: 18, color: Theme.accentOn)
                    .frame(width: 34, height: 34)
                    .background(Circle().fill(Theme.accent))
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.defaultAction)
            .help("Send voice message (Return)")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Theme.list)
    }
}

// MARK: - Video

/// Poster frame + ▶ + length; click opens the gallery.
struct VideoThumbnail: View {
    @EnvironmentObject private var model: AppModel
    let file: FileAttachment
    let messageId: Int64
    let hidden: Bool
    @State private var poster: NSImage?

    var body: some View {
        ZStack {
            if let poster = poster {
                Image(nsImage: poster)
                    .resizable()
                    .scaledToFit()
                    .blur(radius: hidden ? 24 : 0)
            } else {
                Rectangle().fill(Color.black.opacity(0.75)).frame(width: 260, height: 160)
            }
            Image(systemName: "play.fill")
                .font(.system(size: 20, weight: .bold))
                .foregroundColor(.white)
                .frame(width: 48, height: 48)
                .background(Circle().fill(Color.black.opacity(0.5)))
            if let secs = file.duration {
                Text(Format.duration(secs))
                    .font(.cMetaBold)
                    .foregroundColor(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(Color.black.opacity(0.5)))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                    .padding(8)
            }
        }
        .frame(maxWidth: 300, maxHeight: 320)
        .clipShape(RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .task(id: file.path) {
            guard let url = await model.readablePath(file) else { return }
            poster = await Self.poster(url)
        }
    }

    static func poster(_ url: URL) async -> NSImage? {
        await Task.detached(priority: .utility) { () -> NSImage? in
            let generator = AVAssetImageGenerator(asset: AVAsset(url: url))
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = CGSize(width: 600, height: 600)
            guard let cg = try? generator.copyCGImage(at: CMTime(seconds: 0.3, preferredTimescale: 600), actualTime: nil)
            else { return nil }
            return NSImage(cgImage: cg, size: NSSize(width: cg.width, height: cg.height))
        }.value
    }
}

// MARK: - Gallery

/// Photos and videos of the open chat, one at a time: ← / → (arrow keys)
/// step through them, Esc closes (Linux: the swipe gallery).
struct MediaGallery: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    let items: [Message]
    @State var index: Int
    @State private var image: NSImage?
    @State private var player: AVPlayer?
    @State private var loading = true

    private var current: Message? { items.indices.contains(index) ? items[index] : nil }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                IconTool(icon: "cancel", help: "Close (Esc)", size: 16) { close() }
                    .keyboardShortcut(.cancelAction)
                Text(current?.file?.name ?? "")
                    .font(.cBody)
                    .foregroundColor(.white)
                    .lineLimit(1)
                Spacer()
                Text("\(index + 1) of \(items.count)").font(.cMeta).foregroundColor(.white.opacity(0.7))
                if let file = current?.file {
                    IconTool(icon: "download", help: "Save As…", size: 16) { Task { await model.saveAs(file) } }
                    IconTool(icon: "actions-new-window", help: "Open in another app", size: 16) {
                        Task { await model.open(file) }
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            ZStack {
                if let player = player {
                    VideoPlayer(player: player)
                } else if let image = image {
                    Image(nsImage: image).resizable().scaledToFit()
                } else if loading {
                    ProgressView()
                } else {
                    Text("This file can't be shown.").foregroundColor(.white.opacity(0.7))
                }
                HStack {
                    step(-1, icon: "chevron.left", key: .leftArrow)
                    Spacer()
                    step(1, icon: "chevron.right", key: .rightArrow)
                }
                .padding(.horizontal, 10)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 640, idealWidth: 900, minHeight: 480, idealHeight: 680)
        .background(Color.black)
        .task(id: index) { await load() }
        .onDisappear { player?.pause() }
    }

    private func step(_ delta: Int, icon: String, key: KeyEquivalent) -> some View {
        let target = index + delta
        return Button { index = target } label: {
            Image(systemName: icon)
                .font(.system(size: 18, weight: .semibold))
                .foregroundColor(.white)
                .frame(width: 40, height: 40)
                .background(Circle().fill(Color.white.opacity(0.15)))
        }
        .buttonStyle(.plain)
        .keyboardShortcut(key, modifiers: [])
        .opacity(items.indices.contains(target) ? 1 : 0)
        .disabled(!items.indices.contains(target))
    }

    private func load() async {
        player?.pause()
        player = nil
        image = nil
        loading = true
        defer { loading = false }
        guard let file = current?.file, let url = await model.readablePath(file) else { return }
        if file.kind == "video" {
            let p = AVPlayer(url: url)
            player = p
            p.play()
        } else {
            image = NSImage(contentsOf: url)
        }
    }

    private func close() {
        player?.pause()
        model.gallery = nil
        dismiss()
    }
}

/// What the gallery shows: the chat's photos / videos and where to start.
struct GalleryState: Identifiable {
    let id = UUID()
    let items: [Message]
    let index: Int
}
