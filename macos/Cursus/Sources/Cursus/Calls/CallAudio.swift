import AVFoundation
import AudioToolbox
import CCursusBridge
import Foundation

/// Adapts arbitrary microphone buffers to the engine's fixed 20 ms PCM frames.
/// One instance belongs to the input tap; it is never shared with playout.
final class CallCaptureFrames {
    private var samples = [Int16](repeating: 0, count: 960)
    private var used = 0
    private let send: (UnsafeBufferPointer<Int16>) -> Void
    init(send: @escaping (UnsafeBufferPointer<Int16>) -> Void) { self.send = send }
    func append(_ input: UnsafeBufferPointer<Float>) {
        for value in input {
            let finite = value.isFinite ? value : 0
            samples[used] = Int16(max(-32768, min(32767, finite * 32768)))
            used += 1
            if used == samples.count {
                samples.withUnsafeBufferPointer(send)
                used = 0
            }
        }
    }
}

/// Owns resampling on the serial input tap. Exposed separately so the Mac
/// self-test exercises the actual capture conversion without microphone access.
final class CallInputConverter {
    private let converter: AVAudioConverter
    private let output: AVAudioPCMBuffer
    private let capture: CallCaptureFrames
    private let failed: (String) -> Void
    private var reportedError = false
    private let lock = NSLock()
    private var capturedSamples = 0

    init(format: AVAudioFormat, send: @escaping (UnsafeBufferPointer<Int16>) -> Void,
         failed: @escaping (String) -> Void) throws {
        guard let mono = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 1),
              let converter = AVAudioConverter(from: format, to: mono),
              let output = AVAudioPCMBuffer(pcmFormat: mono, frameCapacity: 8192) else {
            throw CoreError(message: "Cannot convert microphone audio to call PCM.")
        }
        self.converter = converter
        self.output = output
        self.capture = CallCaptureFrames(send: send)
        self.failed = failed
    }

    var hasInput: Bool {
        lock.lock(); defer { lock.unlock() }
        return capturedSamples > 0
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        var supplied = false
        for _ in 0..<32 {
            output.frameLength = 0
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, state in
                if supplied { state.pointee = .noDataNow; return nil }
                supplied = true; state.pointee = .haveData; return buffer
            }
            if status == .error {
                if !reportedError { reportedError = true; failed(error?.localizedDescription ?? "Microphone audio conversion failed.") }
                return
            }
            if let data = output.floatChannelData, output.frameLength > 0 {
                capture.append(UnsafeBufferPointer(start: data[0], count: Int(output.frameLength)))
                lock.lock(); capturedSamples += Int(output.frameLength); lock.unlock()
            }
            if status != .haveData || output.frameLength == 0 { break }
        }
    }
}

/// Separate storage for the output render thread; callbacks may request any
/// frame count. AVAudioEngine converts this 48 kHz source to the output device.
private final class CallPlaybackFrames {
    private var samples = [Int16](repeating: 0, count: 960)
    private var offset = 960
    func render(_ output: UnsafeMutablePointer<AudioBufferList>, count: Int) {
        let buffers = UnsafeMutableAudioBufferListPointer(output)
        for index in 0..<count {
            if offset == samples.count {
                let status = samples.withUnsafeMutableBufferPointer { cursus_call_playback($0.baseAddress, 960) }
                if status != 0 { samples.withUnsafeMutableBufferPointer { $0.initialize(repeating: 0) } }
                offset = 0
            }
            let value = Float(samples[offset]) / 32768
            for buffer in buffers {
                if let data = buffer.mData { data.assumingMemoryBound(to: Float.self)[index] = value }
            }
            offset += 1
        }
    }
}

/// All AVAudioEngine setup/teardown is serialized off the UI thread. The tap
/// and render callbacks exchange only PCM with Rust; no signaling runs there.
final class MacCallAudio {
    private let queue = DispatchQueue(label: "chat.cursus.call-audio", qos: .userInitiated)
    private var session: Session?

    func start(failed: @escaping (String) -> Void) {
        queue.async {
            if self.session?.engine.isRunning == true { return }
            self.session?.stop()
            self.session = nil
            self.open(voiceProcessing: true, failed: failed)
        }
    }
    private func open(voiceProcessing: Bool, failed: @escaping (String) -> Void) {
        let current = Session(voiceProcessing: voiceProcessing)
        do {
            try current.start(failed: { error in DispatchQueue.main.async { failed(error) } })
            session = current
            queue.asyncAfter(deadline: .now() + 3) { [weak self, weak current] in
                guard let self = self, let current = current, self.session === current,
                      !current.hasInput else { return }
                current.stop(); self.session = nil
                if voiceProcessing {
                    NSLog("Cursus call: voice processing produced no microphone PCM; retrying normal input")
                    self.open(voiceProcessing: false, failed: failed)
                } else {
                    DispatchQueue.main.async { failed("The microphone is not delivering audio for this call. Check the selected input device.") }
                }
            }
        } catch {
            current.stop()
            if voiceProcessing { open(voiceProcessing: false, failed: failed) }
            else { DispatchQueue.main.async { failed(error.localizedDescription) } }
        }
    }

    func stop() {
        queue.async { self.session?.stop(); self.session = nil }
    }

    private final class Session {
        let engine = AVAudioEngine()
        private var tapped = false
        private var source: AVAudioSourceNode?
        private var sink: AVAudioSinkNode?
        private var capture: CallInputConverter?
        private let voiceProcessing: Bool
        init(voiceProcessing: Bool) { self.voiceProcessing = voiceProcessing }
        var hasInput: Bool { capture?.hasInput == true }

        func start(failed: @escaping (String) -> Void) throws {
            guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
                throw CoreError(message: "Microphone permission is unavailable.")
            }
            let input = engine.inputNode
            // On supported devices Apple's voice-processing I/O supplies AEC,
            // noise reduction and gain control. Other devices use normal I/O.
            if voiceProcessing { try input.setVoiceProcessingEnabled(true) }
            if input.isVoiceProcessingEnabled { input.isVoiceProcessingInputMuted = false }
            let native = input.outputFormat(forBus: 0)
            guard native.sampleRate > 0, native.channelCount > 0,
                  let mono = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 1) else {
                throw CoreError(message: "No usable microphone or speaker is available.")
            }
            let capture = try CallInputConverter(format: native, send: { samples in
                if cursus_call_capture(samples.baseAddress, 960) != 0 {
                    NSLog("Cursus call: microphone PCM could not be encoded")
                }
            }, failed: failed)
            self.capture = capture
            // An explicit sink pulls microphone I/O independently of the
            // speaker's source node. It never routes the mic to the speakers.
            let sink = AVAudioSinkNode { _, _, _ in noErr }
            self.sink = sink
            engine.attach(sink)
            engine.connect(input, to: sink, format: native)
            input.installTap(onBus: 0, bufferSize: 1024, format: native) { buffer, _ in
                capture.append(buffer)
            }
            tapped = true
            let playback = CallPlaybackFrames()
            let node = AVAudioSourceNode(format: mono) { silence, _, count, buffers in
                playback.render(buffers, count: Int(count))
                silence.pointee = false
                return noErr
            }
            source = node
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: mono)
            engine.prepare()
            try engine.start()
        }
        func stop() {
            engine.stop()
            if tapped { engine.inputNode.removeTap(onBus: 0); tapped = false }
            if let source = source { engine.disconnectNodeOutput(source); engine.detach(source); self.source = nil }
            if let sink = sink { engine.disconnectNodeInput(sink); engine.detach(sink); self.sink = nil }
            capture = nil
            engine.reset()
        }
    }
}
