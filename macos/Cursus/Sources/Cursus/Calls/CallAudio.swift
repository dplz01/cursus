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
            let session = Session()
            do { try session.start(); self.session = session }
            catch {
                session.stop(); self.session = nil
                DispatchQueue.main.async { failed(error.localizedDescription) }
            }
        }
    }
    func stop() {
        queue.async { self.session?.stop(); self.session = nil }
    }

    private final class Session {
        let engine = AVAudioEngine()
        private var tapped = false
        private var source: AVAudioSourceNode?
        func start() throws {
            guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
                throw CoreError(message: "Microphone permission is unavailable.")
            }
            let input = engine.inputNode
            // On supported devices Apple's voice-processing I/O supplies AEC,
            // noise reduction and gain control. Other devices use normal I/O.
            try? input.setVoiceProcessingEnabled(true)
            let native = input.outputFormat(forBus: 0)
            guard native.sampleRate > 0, native.channelCount > 0,
                  let mono = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 1),
                  let converter = AVAudioConverter(from: native, to: mono),
                  let converted = AVAudioPCMBuffer(pcmFormat: mono, frameCapacity: 8192) else {
                throw CoreError(message: "No usable microphone or speaker is available.")
            }
            let capture = CallCaptureFrames { samples in
                _ = cursus_call_capture(samples.baseAddress, 960)
            }
            input.installTap(onBus: 0, bufferSize: 1024, format: native) { buffer, _ in
                var supplied = false
                var error: NSError?
                // Drain conversion output even when an input device supplies
                // a larger-than-requested callback (e.g. a Bluetooth mic).
                for _ in 0..<8 {
                    converted.frameLength = 0
                    let status = converter.convert(to: converted, error: &error) { _, state in
                        if supplied { state.pointee = .noDataNow; return nil }
                        supplied = true; state.pointee = .haveData; return buffer
                    }
                    guard status != .error, let data = converted.floatChannelData else { return }
                    capture.append(UnsafeBufferPointer(start: data[0], count: Int(converted.frameLength)))
                    if status != .haveData || converted.frameLength == 0 { break }
                }
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
            engine.reset()
        }
    }
}
