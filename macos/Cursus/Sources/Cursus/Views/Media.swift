import AppKit
import AVFoundation
import AVKit
import ImageIO
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

/// Bars, the played part in `played`, the rest in `rest` — the Linux
/// `wave_layout`: fixed 3pt bars with 2pt gaps on whole device pixels (so
/// edges stay crisp), as many bars as fit, peaks resampled to them (max per
/// bucket). Short bars are dots / capsules (never shorter than wide, so no
/// eye shapes); tall bars get slightly rounded ends.
/// Split into small typed helpers: one big expression was too much for
/// Swift 5.7's checker.
struct WaveformView: View {
    let peaks: [Float]
    var progress: Double = 0
    var played: Color = Theme.accent
    var rest: Color = Theme.mutedText

    @Environment(\.displayScale) private var scale: CGFloat

    var body: some View {
        Canvas { context, size in
            draw(in: &context, size: size)
        }
    }

    private func draw(in context: inout GraphicsContext, size: CGSize) {
        let s: CGFloat = scale > 0 ? scale : 1
        let bars: [CGRect] = WaveformView.layout(peaks: peaks,
                                                 width: (size.width * s).rounded(.down),
                                                 height: (size.height * s).rounded(.down),
                                                 scale: s)
        let count: Double = Double(max(bars.count, 1))
        for (index, px) in bars.enumerated() {
            let rect = CGRect(x: px.minX / s, y: px.minY / s, width: px.width / s, height: px.height / s)
            let isPlayed: Bool = (Double(index) + 1) / count <= progress + 0.001
            let radius: CGFloat = px.height <= px.width * 1.5 ? rect.width / 2 : rect.width * 0.4
            let path = Path(roundedRect: rect, cornerRadius: radius)
            context.fill(path, with: .color(isPlayed ? played : rest))
        }
    }

    /// Bar rectangles in device pixels.
    static func layout(peaks: [Float], width: CGFloat, height: CGFloat, scale: CGFloat) -> [CGRect] {
        guard !peaks.isEmpty, width >= 1, height >= 1 else { return [] }
        let bar: CGFloat = max(1, (3 * scale).rounded())
        let gap: CGFloat = max(1, (2 * scale).rounded())
        let pitch: CGFloat = bar + gap
        let n: Int = max(1, Int(((width + gap) / pitch).rounded(.down)))
        let used: CGFloat = CGFloat(n) * pitch - gap
        let left: CGFloat = max(0, ((width - used) / 2).rounded(.down))
        let src: Int = peaks.count
        var out: [CGRect] = []
        out.reserveCapacity(n)
        for i in 0..<n {
            let a: Int = min(i * src / n, src - 1)
            let b: Int = min(max((i + 1) * src / n, a + 1), src)
            let peak: Float = peaks[a..<b].max() ?? 0
            let level: CGFloat = CGFloat(min(max(peak, 0.08), 1))
            // Never shorter than wide: the quietest bars are round dots.
            var h: CGFloat = max(bar, (level * height * 0.92).rounded())
            if Int(height - h) % 2 != 0 { h += 1 }
            let y: CGFloat = ((height - h) / 2).rounded()
            out.append(CGRect(x: left + CGFloat(i) * pitch, y: y, width: bar, height: h))
        }
        return out
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
        startMetering()
    }

    @MainActor private func startMetering() {
        // Schedule synchronously on the main run loop, including while menus
        // track. The timer target holds the recorder weakly.
        let timer = Timer(timeInterval: 0.1, target: MeterTarget(self),
                          selector: #selector(MeterTarget.fire), userInfo: nil, repeats: true)
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private final class MeterTarget: NSObject {
        private weak var owner: VoiceRecorder?
        init(_ owner: VoiceRecorder) { self.owner = owner }
        @objc func fire() { owner?.tick() }
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
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        // Floats over the chat like the composer it replaces.
        .floatingSurface()
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
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
                CIcon(player.playingId == Self.previewId ? "stop" : "play", size: 16, color: Theme.accentOn)
                    .frame(width: 34, height: 34)
                    .background(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).fill(Theme.accent))
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
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        // Floats over the chat like the composer it replaces.
        .floatingSurface()
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
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
        let size = poster?.size ?? NSSize(width: 260, height: 160)
        let scale = min(1, min(300 / max(1, size.width), 320 / max(1, size.height)))
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
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
                    .padding(8)
            }
        }
        .frame(width: size.width * scale, height: size.height * scale)
        .clipShape(RoundedRectangle(cornerRadius: Radius.mediaInset, style: .continuous))
        .task(id: file.path) {
            guard let url = await model.readablePath(file), !Task.isCancelled else { return }
            let image = await Self.poster(url)
            guard !Task.isCancelled else { return }
            poster = image
        }
    }

    @MainActor
    static func poster(_ url: URL) async -> NSImage? {
        // AppKit image objects stay on the UI actor on every supported macOS.
        // Only immutable encoded bytes cross the worker boundary.
        let bytes: Data? = await Task.detached(priority: .utility) { () -> Data? in
            let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = CGSize(width: 600, height: 600)
            let time = CMTime(seconds: 0.3, preferredTimescale: 600)
            let cg: CGImage
            #if compiler(>=5.7) && CURSUS_MODERN_SHELL
            if #available(macOS 13.0, *) {
                guard let frame = try? await generator.image(at: time) else { return nil }
                cg = frame.image
            } else {
                guard let frame = try? generator.copyCGImage(at: time, actualTime: nil) else { return nil }
                cg = frame
            }
            #else
            guard let frame = try? generator.copyCGImage(at: time, actualTime: nil) else { return nil }
            cg = frame
            #endif
            let bytes = NSMutableData()
            guard let destination = CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil) else { return nil }
            CGImageDestinationAddImage(destination, cg, nil)
            guard CGImageDestinationFinalize(destination) else { return nil }
            return bytes as Data
        }.value
        guard !Task.isCancelled, let bytes = bytes else { return nil }
        return NSImage(data: bytes)
    }
}

// MARK: - Bubble pictures

/// Pictures in chat bubbles. A bubble takes its final size from the
/// sender's embedded preview at once and shows it until the photo is
/// decoded, so the history doesn't jump as pictures load. Photos are
/// decoded off the main thread to a thumbnail and kept, so a chat opened
/// again shows them at once.
@MainActor
enum BubbleImages {
    /// The picture frame in a bubble (Linux / Android fit the same way).
    static let maxSize = CGSize(width: 300, height: 320)

    private static let photos: NSCache<NSString, NSImage> = {
        let cache = NSCache<NSString, NSImage>()
        cache.totalCostLimit = 48 * 1024 * 1024
        return cache
    }()
    private static let previews = NSCache<NSNumber, NSImage>()
    /// Each photo's own size in points (pixels at its DPI, as AppKit shows
    /// it): a picture is never drawn bigger than that, so stickers stay sharp.
    private static var natural: [String: CGSize] = [:]

    static func naturalSize(_ path: String?) -> CGSize? {
        path.flatMap { natural[$0] }
    }

    /// The decoded photo at `path`, if it is already in memory.
    static func cached(_ path: String?) -> NSImage? {
        path.flatMap { photos.object(forKey: $0 as NSString) }
    }

    /// The embedded preview of file `id` (a data URL), decoded once.
    static func preview(id: Int64, dataURL: String?) -> NSImage? {
        let key = NSNumber(value: id)
        if let hit = previews.object(forKey: key) { return hit }
        guard let url = dataURL, let comma = url.range(of: "base64,"),
              let data = Data(base64Encoded: String(url[comma.upperBound...]), options: .ignoreUnknownCharacters),
              let image = NSImage(data: data) else { return nil }
        previews.setObject(image, forKey: key)
        return image
    }

    /// Decode the photo at `url` (a thumbnail big enough for the frame on a
    /// Retina screen) off the main thread, and keep it under `key`.
    static func load(_ url: URL, key: String) async -> NSImage? {
        let decoded: (CGImage, CGSize)? = await Task.detached(priority: .userInitiated) { () -> (CGImage, CGSize)? in
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 900,
            ]
            guard let cg = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
            // Its own size in points: pixels × 72 / DPI, turned like the
            // thumbnail (which has the upright proportions).
            let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any] ?? [:]
            let pw = (props[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue ?? Double(cg.width)
            let ph = (props[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue ?? Double(cg.height)
            let dpi = (props[kCGImagePropertyDPIWidth] as? NSNumber)?.doubleValue ?? 72
            let long = max(pw, ph) * 72 / max(dpi, 1)
            let thumbLong = Double(max(cg.width, cg.height))
            let k = long / max(thumbLong, 1)
            return (cg, CGSize(width: Double(cg.width) * k, height: Double(cg.height) * k))
        }.value
        guard let decoded = decoded else { return nil }
        let (cg, points) = decoded
        natural[key] = points
        let image = NSImage(cgImage: cg, size: NSSize(width: cg.width, height: cg.height))
        photos.setObject(image, forKey: key as NSString, cost: cg.width * cg.height * 4)
        return image
    }

    /// `size`'s proportions fitted into [maxSize], and no bigger than the
    /// photo's `natural` size when that is known.
    static func frame(for size: CGSize, natural: CGSize? = nil) -> CGSize {
        let w = max(1, size.width), h = max(1, size.height)
        var k = min(maxSize.width / w, maxSize.height / h)
        if let natural = natural { k = min(k, natural.width / w) }
        return CGSize(width: (w * k).rounded(), height: (h * k).rounded())
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
                    step(-1, icon: "chevron.left")
                    Spacer()
                    step(1, icon: "chevron.right")
                }
                .padding(.horizontal, 10)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 640, idealWidth: 900, minHeight: 480, idealHeight: 680)
        .background(Color.black)
        .background(GalleryKeys { code in
            if code == 123 { move(-1) }
            else { move(1) }
        })
        .task(id: index) { await load() }
        .onDisappear { player?.pause() }
    }

    private func step(_ delta: Int, icon: String) -> some View {
        let target = index + delta
        return Button { move(delta) } label: {
            Image(systemName: icon)
                .font(.system(size: 18, weight: .semibold))
                .foregroundColor(.white)
                .frame(width: 40, height: 40)
                .background(Circle().fill(Color.white.opacity(0.15)))
        }
        .buttonStyle(.plain)
        .opacity(items.indices.contains(target) ? 1 : 0)
        .disabled(!items.indices.contains(target))
    }

    private func move(_ delta: Int) {
        let next = index + delta
        if items.indices.contains(next) { index = next }
    }

    private func load() async {
        guard !Task.isCancelled else { return }
        let requestedIndex = index
        player?.pause()
        player = nil
        image = nil
        loading = true
        defer { if !Task.isCancelled && index == requestedIndex { loading = false } }
        guard let message = current, let file = message.file,
              let url = await model.readablePath(file), !Task.isCancelled, current?.id == message.id else { return }
        if file.kind == "video" {
            let p = AVPlayer(url: url)
            player = p
            p.play()
        } else {
            let data = await Task.detached(priority: .userInitiated) { try? Data(contentsOf: url) }.value
            guard !Task.isCancelled, current?.id == message.id else { return }
            image = data.flatMap { NSImage(data: $0) }
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

/// Keep arrow navigation registered while media/player views replace focus.
private struct GalleryKeys: NSViewRepresentable {
    let move: (UInt16) -> Void
    func makeNSView(context: Context) -> KeyView { KeyView(move: move) }
    func updateNSView(_ view: KeyView, context: Context) { view.move = move }
    static func dismantleNSView(_ view: KeyView, coordinator: ()) { view.stop() }

    final class KeyView: NSView {
        var move: (UInt16) -> Void
        private var monitor: Any?
        init(move: @escaping (UInt16) -> Void) { self.move = move; super.init(frame: .zero) }
        required init?(coder: NSCoder) { fatalError("Use init(move:)") }
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stop()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self = self, self.window?.isKeyWindow == true,
                      event.modifierFlags.intersection([.command, .control, .option]).isEmpty,
                      event.keyCode == 123 || event.keyCode == 124 else { return event }
                self.move(event.keyCode)
                return nil
            }
        }
        func stop() { if let monitor = monitor { NSEvent.removeMonitor(monitor) }; monitor = nil }
    }
}
