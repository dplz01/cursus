package chat.cursus.media

import android.media.MediaPlayer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** One voice message plays at a time; [playing] / [progress] drive the bubbles. */
object VoicePlayer {
    private var player: MediaPlayer? = null
    var playing by mutableStateOf<Long?>(null); private set
    var progress by mutableStateOf(0f); private set

    fun toggle(fileId: Long, path: String) {
        if (playing == fileId) return stop()
        stop()
        val p = MediaPlayer()
        runCatching {
            p.setDataSource(path)
            p.prepare()
        }.onFailure { p.release(); return }
        p.setOnCompletionListener { stop() }
        p.start()
        player = p
        playing = fileId
        progress = 0f
    }

    /** Called by the UI clock while something plays. */
    fun tick() {
        val p = player ?: return
        runCatching { if (p.duration > 0) progress = p.currentPosition.toFloat() / p.duration }
    }

    fun stop() {
        player?.let { runCatching { it.stop() }; it.release() }
        player = null
        playing = null
        progress = 0f
    }
}
