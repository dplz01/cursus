package chat.cursus.ui.components

import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import chat.cursus.core.Core
import chat.cursus.core.Segment
import chat.cursus.ui.theme.Cursus

/** Display the source markup using the same SimpleX parser as Linux. */
@Composable
fun ProfileText(
    value: String,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    fontWeight: FontWeight? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
    textAlign: TextAlign? = null,
    rich: Boolean = true,
    onOverflow: ((Boolean) -> Unit)? = null,
) {
    val formatted = rememberProfileText(value, style, color, rich)
    Text(formatted, modifier = modifier, style = style, color = color, fontWeight = fontWeight,
        maxLines = maxLines, overflow = overflow, textAlign = textAlign,
        onTextLayout = { layout -> onOverflow?.invoke(layout.hasVisualOverflow) })
}

/**
 * A profile's "about" folded to two lines that end in "… Show more" — an
 * accent link right after the ellipsis — and, unfolded, in "Show less"
 * (Linux `fold`). The kept part keeps its formatting.
 */
@Composable
fun FoldedProfileText(value: String, style: TextStyle, color: Color, open: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val c = Cursus.colors
    val full = rememberProfileText(value, style, color, true)
    val toggle by androidx.compose.runtime.rememberUpdatedState(onToggle)
    val measurer = androidx.compose.ui.text.rememberTextMeasurer()
    androidx.compose.foundation.layout.BoxWithConstraints(modifier) {
        val width = constraints.maxWidth
        val shown = androidx.compose.runtime.remember(full, width, open, style, c.accent) {
            val link = { label: String ->
                buildAnnotatedString {
                    withLink(
                        androidx.compose.ui.text.LinkAnnotation.Clickable(
                            "fold",
                            androidx.compose.ui.text.TextLinkStyles(SpanStyle(color = c.accent, fontWeight = FontWeight.SemiBold)),
                        ) { toggle() },
                    ) { append(label) }
                }
            }
            val limits = androidx.compose.ui.unit.Constraints(maxWidth = width)
            fun fits(t: androidx.compose.ui.text.AnnotatedString) =
                !measurer.measure(t, style, maxLines = 2, constraints = limits).hasVisualOverflow
            when {
                fits(full) -> full
                open -> full + androidx.compose.ui.text.AnnotatedString(" ") + link("Show less")
                else -> {
                    val more = link("Show more")
                    fun head(n: Int) = full.subSequence(0, n).let { h ->
                        h.subSequence(0, h.text.trimEnd().length)
                    }
                    fun candidate(n: Int) = head(n) + androidx.compose.ui.text.AnnotatedString("… ") + more
                    // Longest prefix that still fits with "… Show more".
                    var lo = 0
                    var hi = full.length
                    while (lo < hi) {
                        val mid = (lo + hi + 1) / 2
                        if (fits(candidate(mid))) lo = mid else hi = mid - 1
                    }
                    // End on a whole word when one ends close by.
                    val space = (lo - 1 downTo maxOf(0, lo - 16)).firstOrNull { full.text[it].isWhitespace() }
                    candidate(space ?: lo)
                }
            }
        }
        Text(shown, style = style, color = color)
    }
}

/** The SimpleX-formatted spans of a profile text (same parser as Linux). */
@Composable
private fun rememberProfileText(value: String, style: TextStyle, color: Color, rich: Boolean): androidx.compose.ui.text.AnnotatedString {
    val c = Cursus.colors
    val segments by rememberLoaded<List<Segment>>(emptyList(), value, rich) {
        if (!rich) emptyList() else runCatching { Core.call<List<Segment>>("formatProfileText", mapOf("text" to value)) }.getOrDefault(emptyList())
    }
    return buildAnnotatedString {
        if (segments.isEmpty() || !rich) append(value)
        for (segment in if (rich) segments else emptyList()) {
            val span = when (segment.style) {
                "bold" -> SpanStyle(fontWeight = FontWeight.Bold)
                "italic" -> SpanStyle(fontStyle = FontStyle.Italic)
                "strike" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                "snippet", "command", "secret" -> SpanStyle(fontFamily = FontFamily.Monospace, background = c.bgField)
                "small" -> SpanStyle(fontSize = style.fontSize * .85f)
                "colored" -> SpanStyle(color = when (segment.color) {
                    "red" -> c.danger; "green" -> c.success; "blue" -> c.accent; "yellow" -> c.gold
                    "cyan" -> Color(0xFF00A5B5); "magenta" -> Color(0xFFB0379A)
                    "black" -> Color.Black; "white" -> Color.White; else -> color
                })
                "uri", "email", "phone", "mention" -> SpanStyle(color = c.accent)
                else -> SpanStyle()
            }
            withStyle(span) { append(segment.text) }
        }
    }
}
