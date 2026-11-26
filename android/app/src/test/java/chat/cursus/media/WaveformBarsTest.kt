package chat.cursus.media

import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.sin

/** The Android bars follow the Linux app's `waveform_bars` (media.rs). */
class WaveformBarsTest {
    private fun bars(samples: FloatArray, n: Int, block: Int = 50) =
        Waveforms.bars(Waveforms.Blocks(block).apply { samples.forEach(::add) }, n)

    @Test
    fun ignoresDcOffsetAndNoiseFloor() {
        // Same signal as the Linux test: offset + hiss, with two louder "words".
        val samples = FloatArray(8000) { i ->
            val hiss = ((i * 7919) % 13) / 13f * 0.02f - 0.01f
            val word = if (i in 2000 until 3000 || i in 5000 until 6500) 0.4f * sin(i * 0.3f) else 0f
            0.5f + hiss + word
        }
        val b = bars(samples, 16)
        val full = b.count { it > 0.9f }
        val low = b.count { it < 0.2f }
        assertTrue("only the words should peak: $b", full in 1 until 8)
        assertTrue("pauses should dip: $b", low >= 6)
        assertTrue("flat input → even low line", bars(FloatArray(800) { 0.7f }, 8).all { it <= 0.2f })
    }

    @Test
    fun loudSpeechIsNotAFlatBlock() {
        // Speech-like: the loudness swells and fades, but every bar has a
        // full-scale click in it — per-bar peaks made all of these 1.0.
        val n = 24
        val per = 1000
        val samples = FloatArray(n * per) { i ->
            val bar = i / per
            val level = 0.1f + 0.8f * ((bar * 5) % 9) / 8f
            if (i % per == 0) 1f else level * sin(i * 0.2f)
        }
        val b = bars(samples, n)
        assertTrue("bars must vary with loudness: $b", b.max() - b.min() > 0.5f)
        assertTrue("not mostly at full height: $b", b.count { it > 0.95f } <= n / 4)
    }
}
