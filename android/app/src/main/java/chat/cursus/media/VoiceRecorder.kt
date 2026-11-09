package chat.cursus.media

import android.content.Context
import android.media.MediaRecorder
import android.os.SystemClock
import java.io.File

/** A finished take: an m4a (AAC) file, its length and its loudness bars. */
data class VoiceDraft(val file: File, val durationSecs: Int, val peaks: List<Float>)

/**
 * Voice messages as the official clients send them: AAC in an m4a
 * container, mono. Loudness is sampled while recording for the draft's
 * waveform (received ones are decoded by [Waveforms]).
 */
class VoiceRecorder(private val context: Context) {
    private var recorder: MediaRecorder? = null
    private var file: File? = null
    private var startedAt = 0L
    private val samples = ArrayList<Float>()

    val isRecording get() = recorder != null
    val elapsedSecs get() = if (recorder == null) 0 else ((SystemClock.elapsedRealtime() - startedAt) / 1000).toInt()

    fun start(dir: File) {
        stopQuietly()
        dir.mkdirs()
        val out = File(dir, "voice_${System.currentTimeMillis()}.m4a")
        val r = if (android.os.Build.VERSION.SDK_INT >= 31) MediaRecorder(context) else @Suppress("DEPRECATION") MediaRecorder()
        r.setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
        r.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
        r.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
        r.setAudioChannels(1)
        r.setAudioSamplingRate(44100)
        r.setAudioEncodingBitRate(48000)
        r.setOutputFile(out.absolutePath)
        r.prepare()
        r.start()
        recorder = r
        file = out
        samples.clear()
        startedAt = SystemClock.elapsedRealtime()
    }

    /** Loudness since the last call, 0…1 (call ~10×/s while recording). */
    fun sample(): Float {
        val level = (recorder?.maxAmplitude ?: 0) / 32767f
        samples.add(level)
        return level
    }

    /** Stop and keep the take; null when it was too short or failed. */
    fun stop(): VoiceDraft? {
        val r = recorder ?: return null
        val out = file
        val secs = ((SystemClock.elapsedRealtime() - startedAt) / 1000).toInt()
        recorder = null
        file = null
        val ok = runCatching { r.stop() }.isSuccess
        r.release()
        if (!ok || out == null || secs < 1 || !out.isFile) {
            out?.delete()
            return null
        }
        return VoiceDraft(out, secs, Waveforms.bucket(samples, BARS))
    }

    fun discard() {
        val out = file
        stopQuietly()
        out?.delete()
    }

    private fun stopQuietly() {
        recorder?.let { runCatching { it.stop() }; it.release() }
        recorder = null
        file = null
    }

    companion object {
        /** Same bar count as the Linux app (media::WAVEFORM_BARS). */
        const val BARS = 96
    }
}
