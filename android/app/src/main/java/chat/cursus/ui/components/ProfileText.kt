package chat.cursus.ui.components

import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
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
) {
    val c = Cursus.colors
    val segments by rememberLoaded<List<Segment>>(emptyList(), value, rich) {
        if (!rich) emptyList() else runCatching { Core.call<List<Segment>>("formatProfileText", mapOf("text" to value)) }.getOrDefault(emptyList())
    }
    val formatted = buildAnnotatedString {
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
    Text(formatted, modifier = modifier, style = style, color = color, fontWeight = fontWeight,
        maxLines = maxLines, overflow = overflow, textAlign = textAlign)
}
