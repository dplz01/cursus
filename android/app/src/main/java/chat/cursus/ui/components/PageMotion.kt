package chat.cursus.ui.components

import androidx.activity.BackEventCompat
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.PathEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Phone layout: pages stack and move. Two panes switch pages at once. */
val LocalPhonePages = compositionLocalOf { false }

/** Android's `fast_out_extra_slow_in`, the curve of its page transitions. */
private val ExtraSlowIn = PathEasing(
    Path().apply {
        moveTo(0f, 0f)
        cubicTo(0.05f, 0f, 0.133333f, 0.06f, 0.166666f, 0.4f)
        cubicTo(0.208333f, 0.82f, 0.25f, 1f, 1f, 1f)
    },
)

/** Android 14+ activity transitions: 450 ms; the incoming page fades in
 *  over 83 ms after 50 ms, the closing one fades out over 83 ms after 35 ms. */
private const val TRANSITION_MS = 450
private const val FADE_MS = 83f

private fun fade(f: Float, startMs: Float) = ((f * TRANSITION_MS - startMs) / FADE_MS).coerceIn(0f, 1f)

/**
 * One level of the page stack: [page] opened over [below].
 *
 * On a phone it moves like Android's own pages:
 * - opening: the page comes in from 10% of the width while it fades in,
 *   and the page below moves 10% to the left;
 * - Back button / arrow: the reverse;
 * - predictive back: while the back gesture is held the page shrinks and
 *   follows the finger, uncovering the page below (a peek). Letting go
 *   finishes the move; dragging back cancels it.
 *
 * [page] gets the animated Back to use for its own back arrow. Two panes:
 * no motion, Back closes the page at once.
 */
@Composable
fun StackedPage(
    open: Boolean,
    onBack: () -> Unit,
    below: @Composable () -> Unit,
    page: @Composable (back: () -> Unit) -> Unit,
) {
    val belowState = rememberSaveableStateHolder()
    val belowContent: @Composable () -> Unit = { belowState.SaveableStateProvider("below") { below() } }
    if (!LocalPhonePages.current) {
        BackHandler(enabled = open, onBack = onBack)
        if (open) page(onBack) else belowContent()
        return
    }
    val scope = rememberCoroutineScope()
    var settled by remember { mutableStateOf(false) }
    SideEffect { settled = true }
    // Per opening: 0 → 1 as the page comes in (none on first show).
    val enter = remember(open) { Animatable(if (open && settled) 0f else 1f) }
    // Back arrow: 0 → 1 as the page leaves.
    val close = remember(open) { Animatable(0f) }
    // Predictive back: how far the gesture is (0…1), then the commit fade.
    val gesture = remember(open) { Animatable(0f) }
    val commit = remember(open) { Animatable(0f) }
    var fromLeft by remember { mutableStateOf(true) }
    var leaving by remember(open) { mutableStateOf(false) }

    LaunchedEffect(enter) {
        if (enter.value < 1f) enter.animateTo(1f, tween(TRANSITION_MS, easing = LinearEasing))
    }
    val back: () -> Unit = {
        if (!leaving) {
            leaving = true
            scope.launch {
                close.animateTo(1f, tween(TRANSITION_MS, easing = LinearEasing))
                onBack()
            }
        }
    }
    PredictiveBackHandler(enabled = open && !leaving) { events ->
        var dragged = false
        try {
            events.collect { e ->
                dragged = true
                fromLeft = e.swipeEdge == BackEventCompat.EDGE_LEFT
                gesture.snapTo(e.progress)
            }
            // A Back key / button / the arrow (no gesture): the page closes
            // the way it opened.
            if (!dragged || gesture.value == 0f) {
                back()
                return@PredictiveBackHandler
            }
            leaving = true
            scope.launch {
                commit.animateTo(1f, tween(200, easing = LinearEasing))
                onBack()
            }
        } catch (e: CancellationException) {
            scope.launch { gesture.animateTo(0f, tween(200)) }
        }
    }

    val e = enter.value
    val c = close.value
    val g = gesture.value
    val k = commit.value
    val moving = open && (e < 1f || c > 0f || g > 0f || k > 0f)
    Box(Modifier.fillMaxSize()) {
        // The page below stays composed (hidden) while a page covers it, so
        // going back doesn't have to rebuild it in the first frame.
        val hidden = open && !moving
        Box(
            Modifier.fillMaxSize()
                .then(if (hidden) Modifier.clearAndSetSemantics {}.pointerInput(Unit) {
                    // Nothing under the page takes a touch.
                    awaitPointerEventScope {
                        while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
                    }
                } else Modifier)
                .graphicsLayer {
                    // Not drawn at all while covered.
                    alpha = if (hidden) 0f else 1f
                    // Opening pushes it 10% left; closing brings it back.
                    translationX = when {
                        e < 1f -> -0.1f * size.width * ExtraSlowIn.transform(e)
                        c > 0f -> -0.1f * size.width * (1f - ExtraSlowIn.transform(c))
                        else -> 0f
                    }
                },
        ) { belowContent() }
        if (g > 0f || k > 0f) {
            // The page below comes out of the shade as the gesture goes on.
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.3f * (1f - g) * (1f - k))))
        }
        if (open) {
            Box(
                Modifier.fillMaxSize().graphicsLayer {
                    val w = size.width
                    when {
                        e < 1f -> {
                            translationX = 0.1f * w * (1f - ExtraSlowIn.transform(e))
                            alpha = fade(e, 50f)
                        }
                        c > 0f -> {
                            translationX = 0.1f * w * ExtraSlowIn.transform(c)
                            alpha = 1f - fade(c, 35f)
                        }
                        g > 0f || k > 0f -> {
                            // Shrinks to 90% and drifts away from the edge
                            // the swipe started from, with rounded corners.
                            val shift = (w / 20f - 8.dp.toPx()).coerceAtLeast(0f)
                            scaleX = 1f - 0.1f * g
                            scaleY = scaleX
                            translationX = (if (fromLeft) 1f else -1f) * shift * g
                            shape = RoundedCornerShape(32.dp * g)
                            clip = true
                            alpha = 1f - k
                        }
                    }
                },
            ) { page(back) }
        }
    }
}
