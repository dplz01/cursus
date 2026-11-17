package chat.cursus.media

import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import java.nio.ByteOrder
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.abs
import kotlin.math.max

/** Loudness bars of a voice message, decoded from the audio itself (cached). */
object Waveforms {
    private val cache = ConcurrentHashMap<String, List<Float>>()

    fun cached(path: String): List<Float>? = cache[path]

    /** Blocking: decode [path] and return [VoiceRecorder.BARS] bars (0…1). */
    fun load(path: String): List<Float>? {
        cache[path]?.let { return it }
        val peaks = runCatching { decode(path) }.getOrNull() ?: return null
        cache[path] = peaks
        return peaks
    }

    private fun decode(path: String): List<Float> {
        val ex = MediaExtractor()
        ex.setDataSource(path)
        val track = (0 until ex.trackCount).first {
            ex.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true
        }
        ex.selectTrack(track)
        val format = ex.getTrackFormat(track)
        val codec = MediaCodec.createDecoderByType(format.getString(MediaFormat.KEY_MIME)!!)
        codec.configure(format, null, null, 0)
        codec.start()
        // One peak per ~20 ms block, bucketed into bars at the end.
        val rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        val channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        val block = max(1, rate * channels / 50)
        val blocks = ArrayList<Float>()
        var peak = 0
        var count = 0
        val info = MediaCodec.BufferInfo()
        var inputDone = false
        var outputDone = false
        while (!outputDone) {
            if (!inputDone) {
                val i = codec.dequeueInputBuffer(10_000)
                if (i >= 0) {
                    val buf = codec.getInputBuffer(i)!!
                    val n = ex.readSampleData(buf, 0)
                    if (n < 0) {
                        codec.queueInputBuffer(i, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        inputDone = true
                    } else {
                        codec.queueInputBuffer(i, 0, n, ex.sampleTime, 0)
                        ex.advance()
                    }
                }
            }
            val o = codec.dequeueOutputBuffer(info, 10_000)
            if (o >= 0) {
                val shorts = codec.getOutputBuffer(o)!!.order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
                while (shorts.hasRemaining()) {
                    peak = max(peak, abs(shorts.get().toInt()))
                    if (++count == block) {
                        blocks.add(peak / 32768f); peak = 0; count = 0
                    }
                }
                codec.releaseOutputBuffer(o, false)
                if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outputDone = true
            }
        }
        codec.stop(); codec.release(); ex.release()
        return bucket(blocks, VoiceRecorder.BARS)
    }

    /** Max of each of [bars] equal slices, scaled so the loudest bar is 1. */
    fun bucket(values: List<Float>, bars: Int): List<Float> {
        if (values.isEmpty()) return List(bars) { 0.05f }
        val out = List(bars) { b ->
            val from = b * values.size / bars
            val to = max(from + 1, (b + 1) * values.size / bars).coerceAtMost(values.size)
            if (from >= values.size) 0f else values.subList(from, to).max()
        }
        val top = out.max().takeIf { it > 0f } ?: 1f
        return out.map { (it / top).coerceIn(0.05f, 1f) }
    }
}
