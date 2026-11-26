package chat.cursus.media

import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import java.nio.ByteOrder
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sqrt

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
        // Sums per ~20 ms block, turned into bars at the end.
        val rate = format.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        val channels = format.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        val blocks = Blocks(max(1, rate * channels / 50))
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
                while (shorts.hasRemaining()) blocks.add(shorts.get() / 32768f)
                codec.releaseOutputBuffer(o, false)
                if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) outputDone = true
            }
        }
        codec.stop(); codec.release(); ex.release()
        return bars(blocks, VoiceRecorder.BARS)
    }

    /** Running sums of samples in blocks of [size] (no PCM kept). */
    class Blocks(private val size: Int) {
        internal val sum = ArrayList<Double>()
        internal val sumSq = ArrayList<Double>()
        internal val count = ArrayList<Int>()
        private var s = 0.0
        private var q = 0.0
        private var n = 0

        fun add(x: Float) {
            s += x; q += x.toDouble() * x; n++
            if (n == size) flush()
        }

        internal fun flush() {
            if (n == 0) return
            sum.add(s); sumSq.add(q); count.add(n)
            s = 0.0; q = 0.0; n = 0
        }
    }

    /**
     * Loudness bars (0.05…1), the same as the Linux app's `waveform_bars`:
     * per-bar RMS with the DC offset removed, the noise floor (10th
     * percentile) subtracted, and a gentle curve so speech reads as peaks
     * and pauses as dips. (Each bar's *peak* scaled to the loudest made
     * nearly every bar of normal speech full height: a flat block.)
     */
    fun bars(blocks: Blocks, bars: Int): List<Float> {
        blocks.flush()
        val minBar = 0.05f
        val total = blocks.count.sum()
        if (total == 0 || bars < 1) return List(max(bars, 1)) { minBar }
        val mean = blocks.sum.sum() / total
        val n = blocks.count.size
        val rms = List(bars) { b ->
            val from = (b.toLong() * n / bars).toInt().coerceAtMost(n - 1)
            val to = max(from + 1, ((b + 1).toLong() * n / bars).toInt()).coerceAtMost(n)
            var s = 0.0; var q = 0.0; var c = 0
            for (i in from until to) { s += blocks.sum[i]; q += blocks.sumSq[i]; c += blocks.count[i] }
            // Σ(x − mean)² = Σx² − 2·mean·Σx + c·mean²
            sqrt(((q - 2 * mean * s + c * mean * mean) / c).coerceAtLeast(0.0)).toFloat()
        }
        val sorted = rms.sorted()
        val floor = sorted[sorted.size / 10]
        val top = sorted.last()
        val span = top - floor
        if (span <= 1e-5f) {
            // Flat signal (silence or constant hum): a low, even line.
            return List(bars) { if (top > 1e-4f) 0.2f else minBar }
        }
        return rms.map { ((it - floor) / span).coerceIn(0f, 1f).pow(0.7f).coerceAtLeast(minBar) }
    }
}
