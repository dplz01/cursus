package chat.cursus.media

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaMuxer
import android.media.MediaRecorder
import android.os.SystemClock
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.sqrt
import kotlin.math.tanh

/** A finished take: an m4a (AAC) file, its length and its loudness bars. */
data class VoiceDraft(val file: File, val durationSecs: Int, val peaks: List<Float>)

/**
 * Voice messages as the official clients play them: AAC-LC in m4a, mono.
 *
 * The phone's call-style processing (`VOICE_COMMUNICATION`: noise
 * suppression + AGC) made takes quiet and muffled, so this records the raw
 * microphone and treats the take itself: rumble filter, loudness brought to
 * a steady level, soft limiter instead of clipping. 24 kHz / 40 kbps keeps
 * speech clear while ~100 s still fit the 510 KB the clients auto-download.
 */
class VoiceRecorder {
    private var record: AudioRecord? = null
    private var thread: Thread? = null
    private var raw: File? = null
    private var rate = RATE
    private var startedAt = 0L
    @Volatile private var level = 0f
    @Volatile private var running = false
    private val samples = ArrayList<Float>()

    val isRecording get() = running
    val elapsedSecs get() = if (!running) 0 else ((SystemClock.elapsedRealtime() - startedAt) / 1000).toInt()

    @SuppressLint("MissingPermission") // asked for by the composer before starting
    fun start(dir: File) {
        discard()
        dir.mkdirs()
        rate = RATE
        var min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) {
            rate = 16000
            min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        }
        val r = AudioRecord(
            MediaRecorder.AudioSource.MIC, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, rate),
        )
        check(r.state == AudioRecord.STATE_INITIALIZED) { "microphone unavailable" }
        val out = File(dir, "take_${System.currentTimeMillis()}.pcm")
        record = r
        raw = out
        samples.clear()
        running = true
        r.startRecording()
        startedAt = SystemClock.elapsedRealtime()
        thread = Thread({
            val buf = ShortArray(rate / 20) // 50 ms
            BufferedOutputStream(FileOutputStream(out)).use { os ->
                val bytes = ByteBuffer.allocate(buf.size * 2).order(ByteOrder.LITTLE_ENDIAN)
                while (running) {
                    val n = r.read(buf, 0, buf.size)
                    if (n <= 0) continue
                    var peak = 0
                    bytes.clear()
                    for (i in 0 until n) {
                        bytes.putShort(buf[i])
                        peak = maxOf(peak, abs(buf[i].toInt()))
                    }
                    os.write(bytes.array(), 0, n * 2)
                    level = peak / 32768f
                }
            }
        }, "cursus-voice").apply { start() }
    }

    /** Loudness of the last 50 ms, 0…1 (call ~10×/s while recording). */
    fun sample(): Float {
        val l = level
        samples.add(l)
        return l
    }

    /**
     * Stop and turn the take into a draft (blocking: filter, level, encode).
     * Null when it was too short or failed.
     */
    fun stop(): VoiceDraft? {
        if (!running) return null
        val secs = ((SystemClock.elapsedRealtime() - startedAt) / 1000).toInt()
        val pcm = raw
        halt()
        if (pcm == null || secs < 1) {
            pcm?.delete()
            return null
        }
        return try {
            val audio = readPcm(pcm)
            process(audio, rate)
            val m4a = File(pcm.parentFile, pcm.nameWithoutExtension + ".m4a")
            encodeAac(audio, rate, m4a)
            VoiceDraft(m4a, secs, Waveforms.bucket(blockPeaks(audio, rate), BARS))
        } catch (_: Exception) {
            null
        } finally {
            pcm.delete()
        }
    }

    fun discard() {
        val pcm = raw
        halt()
        pcm?.delete()
    }

    private fun halt() {
        running = false
        thread?.join(500)
        thread = null
        record?.let { runCatching { it.stop() }; it.release() }
        record = null
        raw = null
    }

    companion object {
        /** Same bar count as the Linux app (media::WAVEFORM_BARS). */
        const val BARS = 96
        private const val RATE = 24000
        private const val BITRATE = 40000
        /** Speech level the take is brought to (RMS of its voiced parts, about −18 dBFS). */
        private const val TARGET_RMS = 0.125f
        private const val MAX_GAIN = 12f

        private fun readPcm(file: File): FloatArray {
            val n = (file.length() / 2).toInt()
            val out = FloatArray(n)
            DataInputStream(BufferedInputStream(FileInputStream(file))).use { input ->
                for (i in 0 until n) {
                    val lo = input.read()
                    val hi = input.read()
                    out[i] = ((hi shl 8) or lo).toShort() / 32768f
                }
            }
            return out
        }

        /** 1-pole high-pass at ~80 Hz: removes DC and handling / wind rumble, in place. */
        fun highPass(x: FloatArray, rate: Int) {
            val rc = 1f / (2f * Math.PI.toFloat() * 80f)
            val a = rc / (rc + 1f / rate)
            var prevIn = 0f
            var prevOut = 0f
            for (i in x.indices) {
                val y = a * (prevOut + x[i] - prevIn)
                prevIn = x[i]; prevOut = y; x[i] = y
            }
        }

        /** Rumble / DC filter, loudness to [TARGET_RMS], soft limiter — in place. */
        fun process(x: FloatArray, rate: Int) {
            highPass(x, rate)
            // Level from the louder half of 20 ms frames (speech, not pauses).
            val frame = rate / 50
            val rms = (0 until x.size / frame).map { f ->
                var s = 0.0
                for (i in f * frame until (f + 1) * frame) s += x[i] * x[i]
                sqrt(s / frame).toFloat()
            }.sortedDescending()
            val voiced = rms.take(maxOf(1, rms.size / 2))
            val speech = if (voiced.isEmpty()) 0f else voiced.average().toFloat()
            val gain = if (speech > 1e-4f) (TARGET_RMS / speech).coerceIn(1f, MAX_GAIN) else 1f
            // Gain, then a soft knee above 0.7 so peaks round off instead of clipping.
            for (i in x.indices) {
                val v = x[i] * gain
                val m = abs(v)
                x[i] = if (m <= 0.7f) v else Math.copySign(0.7f + 0.29f * tanh((m - 0.7f) / 0.29f), v)
            }
        }

        private fun blockPeaks(x: FloatArray, rate: Int): List<Float> {
            val block = rate / 50
            return (0 until x.size / block).map { b ->
                var p = 0f
                for (i in b * block until (b + 1) * block) p = maxOf(p, abs(x[i]))
                p
            }
        }

        private fun encodeAac(x: FloatArray, rate: Int, out: File) {
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, rate, 1).apply {
                setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                setInteger(MediaFormat.KEY_BIT_RATE, BITRATE)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 16384)
            }
            val codec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC)
            codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            codec.start()
            val muxer = MediaMuxer(out.absolutePath, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
            var track = -1
            var pos = 0
            var inputDone = false
            val info = MediaCodec.BufferInfo()
            try {
                while (true) {
                    if (!inputDone) {
                        val i = codec.dequeueInputBuffer(10_000)
                        if (i >= 0) {
                            val buf = codec.getInputBuffer(i)!!.order(ByteOrder.LITTLE_ENDIAN)
                            buf.clear()
                            val count = minOf(buf.capacity() / 2, x.size - pos)
                            for (k in 0 until count) buf.putShort((x[pos + k] * 32767f).toInt().coerceIn(-32768, 32767).toShort())
                            val pts = pos * 1_000_000L / rate
                            pos += count
                            if (pos >= x.size) {
                                codec.queueInputBuffer(i, 0, count * 2, pts, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                                inputDone = true
                            } else {
                                codec.queueInputBuffer(i, 0, count * 2, pts, 0)
                            }
                        }
                    }
                    val o = codec.dequeueOutputBuffer(info, 10_000)
                    when {
                        o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                            track = muxer.addTrack(codec.outputFormat)
                            muxer.start()
                        }
                        o >= 0 -> {
                            val data = codec.getOutputBuffer(o)!!
                            if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) info.size = 0
                            if (info.size > 0 && track >= 0) muxer.writeSampleData(track, data, info)
                            codec.releaseOutputBuffer(o, false)
                            if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) break
                        }
                    }
                }
            } finally {
                runCatching { codec.stop() }
                codec.release()
                runCatching { if (track >= 0) muxer.stop() }
                muxer.release()
            }
        }
    }
}
