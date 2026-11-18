package chat.cursus.calls

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AudioEffect
import android.media.audiofx.NoiseSuppressor
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.Process
import androidx.core.content.ContextCompat
import chat.cursus.core.Native
import java.util.concurrent.atomic.AtomicBoolean

/** Android owns the audio clock; Rust owns Opus, jitter buffering and encryption. */
class CallAudio(private val context: Context, private val failed: (String) -> Unit) : AutoCloseable {
    private val manager = context.getSystemService(AudioManager::class.java)
    private val running = AtomicBoolean(false)
    private val effects = mutableListOf<AudioEffect>()
    private var record: AudioRecord? = null
    private var track: AudioTrack? = null
    private var capture: Thread? = null
    private var playback: Thread? = null
    private val previousMode = manager.mode
    @Suppress("DEPRECATION")
    private val previousSpeaker = manager.isSpeakerphoneOn
    private var ownsFocus = false
    private val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
        .setAudioAttributes(attributes).setOnAudioFocusChangeListener({ change ->
            if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                fail("Call ended because another app needs the audio device.")
            }
        }, Handler(Looper.getMainLooper())).build()

    fun start() {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            throw SecurityException("Allow microphone access to make a call.")
        }
        check(manager.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) { "The audio device is in use." }
        ownsFocus = true
        try {
            manager.mode = AudioManager.MODE_IN_COMMUNICATION
            val inputBytes = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            check(inputBytes > 0) { "48 kHz microphone audio is unavailable." }
            val mic = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
                .setAudioFormat(format(AudioFormat.CHANNEL_IN_MONO)).setBufferSizeInBytes(maxOf(inputBytes, FRAME * 4)).build()
            record = mic
            check(mic.state == AudioRecord.STATE_INITIALIZED) { "The microphone could not start." }
            if (AcousticEchoCanceler.isAvailable()) enableEffect { AcousticEchoCanceler.create(mic.audioSessionId) }
            if (NoiseSuppressor.isAvailable()) enableEffect { NoiseSuppressor.create(mic.audioSessionId) }
            val outputBytes = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
            check(outputBytes > 0) { "48 kHz speaker audio is unavailable." }
            val speaker = AudioTrack.Builder().setAudioAttributes(attributes).setAudioFormat(format(AudioFormat.CHANNEL_OUT_MONO))
                .setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(maxOf(outputBytes, FRAME * 4)).build()
            track = speaker
            check(speaker.state == AudioTrack.STATE_INITIALIZED) { "The speaker could not start." }
            mic.startRecording()
            check(mic.recordingState == AudioRecord.RECORDSTATE_RECORDING) { "The microphone is unavailable." }
            speaker.play()
            running.set(true)
            capture = worker("call-microphone") {
                val pcm = ShortArray(FRAME)
                while (running.get()) {
                    var offset = 0
                    while (running.get() && offset < FRAME) {
                        val read = mic.read(pcm, offset, FRAME - offset, AudioRecord.READ_BLOCKING)
                        check(read > 0) { "The microphone disconnected." }
                        offset += read
                    }
                    if (running.get()) Native.captureCallAudio(pcm)
                }
            }
            playback = worker("call-speaker") {
                val pcm = ShortArray(FRAME)
                while (running.get()) {
                    Native.playCallAudio(pcm)
                    var offset = 0
                    while (running.get() && offset < FRAME) {
                        val written = speaker.write(pcm, offset, FRAME - offset, AudioTrack.WRITE_BLOCKING)
                        check(written > 0) { "The audio output disconnected." }
                        offset += written
                    }
                }
            }
        } catch (e: Exception) { close(); throw e }
    }

    @Suppress("DEPRECATION")
    fun speaker(on: Boolean) {
        if (Build.VERSION.SDK_INT >= 31) {
            if (on) {
                val device = manager.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                    ?: error("No speaker is available.")
                check(manager.setCommunicationDevice(device)) { "Could not switch audio output." }
            } else manager.clearCommunicationDevice() // Restore headset/Bluetooth/earpiece routing.
        } else manager.isSpeakerphoneOn = on
    }

    // Some vendors advertise effects but cannot give this session control.
    // VOICE_COMMUNICATION still supplies the platform's default processing.
    private fun enableEffect(create: () -> AudioEffect?) {
        val effect = runCatching(create).getOrNull() ?: return
        if (runCatching { effect.enabled = true; effect.enabled }.getOrDefault(false)) effects += effect
        else runCatching { effect.release() }
    }

    private fun worker(name: String, block: () -> Unit) = Thread({
        Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
        try { block() } catch (e: Exception) { fail(e.message ?: "Call audio failed.") }
    }, name).apply { start() }

    private fun fail(message: String) { if (running.getAndSet(false)) failed(message) }

    /** Run off the UI thread: stop unblocks read/write before joining workers. */
    @Suppress("DEPRECATION")
    override fun close() {
        running.set(false)
        runCatching { record?.stop() }
        runCatching { track?.pause(); track?.flush() }
        capture?.join(1000); playback?.join(1000)
        effects.forEach { runCatching { it.release() } }; effects.clear()
        record?.release(); record = null
        track?.release(); track = null
        if (ownsFocus) {
            if (Build.VERSION.SDK_INT >= 31) manager.clearCommunicationDevice() else manager.isSpeakerphoneOn = previousSpeaker
            manager.mode = previousMode
            manager.abandonAudioFocusRequest(focus)
            ownsFocus = false
        }
    }

    private fun format(channels: Int) = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT)
        .setSampleRate(RATE).setChannelMask(channels).build()
    companion object { const val RATE = 48_000; const val FRAME = 960 }
}
