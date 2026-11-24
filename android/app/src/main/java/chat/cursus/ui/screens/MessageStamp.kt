package chat.cursus.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.core.Message
import chat.cursus.core.Stamps
import chat.cursus.ui.theme.Cursus

@Composable
private fun stampStyle() = Cursus.type.meta.copy(fontSize = 11.sp, lineHeight = 14.sp)

@Composable
internal fun MessageStamp(message: Message, showReceipt: Boolean, modifier: Modifier = Modifier, onMedia: Boolean = false) {
    val c = Cursus.colors
    val style = stampStyle()
    val foreground = if (onMedia) Color.White else if (message.outgoing) c.bubbleOutMeta else c.textMuted
    val surface = if (onMedia) modifier.background(Color.Black.copy(alpha = 0.5f), RoundedCornerShape(8.dp))
        .padding(horizontal = 6.dp, vertical = 3.dp) else modifier
    Row(surface, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (message.edited) Text("edited", style = style, color = if (onMedia) Color.White else c.textMuted)
        Text(Stamps.time(message.createdAt), style = style, color = foreground)
        if (message.outgoing && showReceipt) DeliveryGlyph(message.delivery, c)
    }
}

/** An indivisible reservation wraps with the text; metadata stays bottom-right.
 * Measure using the same fonts, density and receipt size as the visible row.
 * Links, secret spans, emoji and explicit newlines still use Compose's shaper.
 */
@Composable
internal fun StampedMessageText(text: AnnotatedString, style: TextStyle, message: Message, showReceipt: Boolean) {
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    val metaStyle = stampStyle()
    val labels = buildList {
        if (message.edited) add("edited")
        add(Stamps.time(message.createdAt))
    }
    val measurements = labels.map { measurer.measure(AnnotatedString(it), metaStyle) }
    val receipt = message.outgoing && showReceipt
    val (stampWidth, stampHeight) = with(density) {
        val width = measurements.sumOf { it.size.width } + (if (receipt) 14.dp.toPx() else 0f) +
            (labels.size + (if (receipt) 1 else 0) - 1) * 4.dp.toPx()
        val height = maxOf(measurements.maxOf { it.size.height }.toFloat(),
            if (receipt) 14.dp.toPx() else 0f)
        width.coerceAtLeast(1f).toSp() to height.toSp()
    }
    val reserved = buildAnnotatedString {
        append(text)
        append(' ')
        appendInlineContent("stamp", " ")
    }
    Box(Modifier.fillMaxWidth()) {
        Text(
            reserved, style = style, color = Cursus.colors.textPrimary,
            inlineContent = mapOf("stamp" to InlineTextContent(
                // A full body-line reservation would enlarge the line and lift its glyphs.
                Placeholder(stampWidth, stampHeight, PlaceholderVerticalAlign.TextBottom),
            ) {}),
        )
        MessageStamp(message, showReceipt, Modifier.align(Alignment.BottomEnd))
    }
}
