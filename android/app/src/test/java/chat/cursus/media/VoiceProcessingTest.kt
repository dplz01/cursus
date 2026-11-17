package chat.cursus.media

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin
import kotlin.math.sqrt

class VoiceProcessingTest {
    private val rate = 24000

    private fun tone(freq: Double, amp: Double, secs: Double = 1.0) =
        FloatArray((rate * secs).toInt()) { (amp * sin(2 * PI * freq * it / rate)).toFloat() }

    private fun rms(x: FloatArray, from: Int = rate / 10) =
        sqrt(x.drop(from).sumOf { it.toDouble() * it } / (x.size - from)).toFloat()

    @Test
    fun quietSpeechIsBroughtUp() {
        val x = tone(300.0, 0.02) // about −37 dBFS, a quiet phone take
        VoiceRecorder.process(x, rate)
        assertEquals(0.125f, rms(x), 0.02f)
    }

    @Test
    fun loudTakeNeverClips() {
        val x = tone(300.0, 0.99)
        VoiceRecorder.process(x, rate)
        assertTrue(x.all { abs(it) < 1f })
    }

    @Test
    fun rumbleIsFilteredSpeechKept() {
        val low = tone(20.0, 0.3)
        val voice = tone(1000.0, 0.3)
        VoiceRecorder.highPass(low, rate)
        VoiceRecorder.highPass(voice, rate)
        assertTrue(rms(low) < 0.3f * 0.707f * 0.3f) // ≥ 10 dB down
        assertEquals(0.3f * 0.707f, rms(voice), 0.01f)
    }

    @Test
    fun silenceStaysSilent() {
        val x = FloatArray(rate)
        VoiceRecorder.process(x, rate)
        assertTrue(x.all { it == 0f })
    }
}
