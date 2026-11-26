package chat.cursus.ui.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp

/**
 * Page transitions (Linux `page_motion`): a newly opened page — chat,
 * profile, a profile's settings, a settings category — slides in from the
 * right. Opened from the list on one column it travels the full width over
 * the list; otherwise it moves a short way while it fades in. Back and
 * destination switches are instant.
 */
class PageMotion(val remaining: Float, val full: Boolean)

private class PageTrail {
    var key: Any? = null
    var depth = 0
    var seen = false
}

/**
 * Call where it stays composed while pages change (so it remembers the
 * previous page). [key] names the page, [depth] how deep it is (0 = the
 * list), [onePane] whether pages cover the list.
 */
@Composable
fun rememberPageMotion(key: Any?, depth: Int, onePane: Boolean = false): PageMotion {
    val trail = remember { PageTrail() }
    val (forward, full) = remember(key) {
        val forward = trail.seen && key != trail.key && depth > 0 && depth >= trail.depth
        val full = onePane && trail.depth == 0
        trail.key = key
        trail.depth = depth
        trail.seen = true
        forward to full
    }
    val anim = remember(key) { Animatable(if (forward) 1f else 0f) }
    LaunchedEffect(anim) {
        if (anim.value > 0f) anim.animateTo(0f, tween(if (full) 300 else 220, easing = EaseOut))
    }
    return PageMotion(anim.value, full)
}

private val EaseOut = CubicBezierEasing(0.33f, 1f, 0.68f, 1f)

/** Draw this page where [motion] has it: shifted right (and faded in when short). */
fun Modifier.pageSlide(motion: PageMotion): Modifier =
    if (motion.remaining <= 0f) this else graphicsLayer {
        translationX = motion.remaining * if (motion.full) size.width else 48.dp.toPx()
        if (!motion.full) alpha = 1f - motion.remaining
    }
