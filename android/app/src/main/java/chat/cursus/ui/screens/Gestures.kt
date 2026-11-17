package chat.cursus.ui.screens

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged

/**
 * Pinch-zoom and pan once zoomed; at 1× single-finger drags pass through to
 * the pager so swiping between photos still works.
 */
internal fun Modifier.pointerInputZoomImpl(scale: Float, onChange: (Float, Offset) -> Unit): Modifier =
    pointerInput(Unit) {
        var s = 1f
        var o = Offset.Zero
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false)
            do {
                val event = awaitPointerEvent()
                val fingers = event.changes.count { it.pressed }
                if (fingers > 1 || s > 1f) {
                    s = (s * event.calculateZoom()).coerceIn(1f, 5f)
                    o = if (s == 1f) Offset.Zero else o + event.calculatePan()
                    onChange(s, o)
                    event.changes.forEach { if (it.positionChanged()) it.consume() }
                }
            } while (event.changes.any { it.pressed })
        }
    }
