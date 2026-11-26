package chat.cursus.ui.components

import android.os.Build
import androidx.compose.foundation.border
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlurEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.layer.GraphicsLayer
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import chat.cursus.ui.theme.Cursus

/**
 * Backdrop blur for the floating composer and destination bar — Android's
 * counterpart of the macOS material. The content that scrolls underneath
 * (the chat history, the chat list) draws itself into a [GraphicsLayer]
 * as well as on screen; each glass surface redraws the part of that layer
 * behind it through a RenderEffect blur, clipped to its own shape. Only
 * the surface is frosted: nothing is drawn around it.
 *
 * RenderEffect needs Android 12 (API 31). Before that, and where nothing
 * provides a backdrop, the surface is a dense frosted fill instead.
 */
class Backdrop internal constructor(internal val layer: GraphicsLayer) {
    /** Where the recorded content sits in the window. */
    internal var origin by mutableStateOf(Offset.Zero)
    /** The colour under the content (recorded content can be transparent). */
    internal var base by mutableStateOf(Color.Transparent)
}

val LocalBackdrop = compositionLocalOf<Backdrop?> { null }

@Composable
fun rememberBackdrop(): Backdrop {
    val layer = rememberGraphicsLayer()
    return remember(layer) { Backdrop(layer) }
}

/** Marks the content that glass surfaces blur. It must not contain them. */
fun Modifier.backdropSource(backdrop: Backdrop, base: Color): Modifier = this
    .onGloballyPositioned { backdrop.origin = it.positionInRoot(); backdrop.base = base }
    .drawWithContent {
        backdrop.layer.record { this@drawWithContent.drawContent() }
        drawLayer(backdrop.layer)
    }

private val BLUR_SUPPORTED = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

/** Blur radius of the backdrop (macOS `.regularMaterial` is about 30pt). */
private val BLUR_RADIUS = 24.dp

/** Soft lift; the shape's own edge does most of the separating. */
private val GLASS_ELEVATION = 3.dp

/**
 * A floating glass surface: a subtle shadow, the blurred backdrop clipped
 * to [shape], a translucent tint and a thin light edge.
 */
@Composable
fun Modifier.glass(shape: Shape): Modifier {
    val c = Cursus.colors
    val backdrop = LocalBackdrop.current
    val blurring = BLUR_SUPPORTED && backdrop != null
    // Over a real blur the tint can be light; without one it has to be
    // dense, or text sliding underneath stays legible through it.
    val tint = c.bgElevated.copy(alpha = when {
        blurring -> if (c.isDark) 0.62f else 0.70f
        c.isDark -> 0.94f
        else -> 0.95f
    })
    val edge = if (c.isDark) Color.White.copy(alpha = 0.12f) else Color.Black.copy(alpha = 0.08f)
    var position by remember { mutableStateOf(Offset.Zero) }
    val blurLayer = rememberGraphicsLayer()
    val radiusPx = with(LocalDensity.current) { BLUR_RADIUS.toPx() }
    return this
        .shadow(GLASS_ELEVATION, shape, clip = false,
            ambientColor = Color.Black.copy(alpha = 0.25f), spotColor = Color.Black.copy(alpha = 0.3f))
        .onGloballyPositioned { position = it.positionInRoot() }
        .clip(shape)
        .drawBehind {
            if (blurring && backdrop != null) {
                blurLayer.renderEffect = BlurEffect(radiusPx, radiusPx, TileMode.Clamp)
                blurLayer.record {
                    drawRect(backdrop.base)
                    translate(backdrop.origin.x - position.x, backdrop.origin.y - position.y) {
                        drawLayer(backdrop.layer)
                    }
                }
                drawLayer(blurLayer)
            }
            drawRect(tint)
        }
        .border(1.dp, edge, shape)
}
