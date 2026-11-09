package chat.cursus.ui.components

import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import chat.cursus.R
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

// The complete button set of docs/DESIGN_SYSTEM.md § Buttons. Screens use
// these; they never style a button themselves.

@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = Cursus.colors
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val fill = when {
        !enabled -> c.disabledFill
        pressed -> c.accentPress
        else -> c.accent
    }
    Box(
        modifier
            .clip(Radius.md)
            .background(fill)
            .clickable(enabled = enabled, interactionSource = source, indication = null, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Cursus.type.body, color = if (enabled) c.accentOn else c.textMuted, fontWeight = FontWeight.Medium)
    }
}

@Composable
fun SecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = Cursus.colors
    Box(
        modifier
            .clip(Radius.md)
            .background(c.bgSidebar)
            .border(1.dp, c.border, Radius.md)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Cursus.type.body, color = if (enabled) c.textPrimary else c.textMuted)
    }
}

@Composable
fun DangerButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = Cursus.colors
    Box(
        modifier.clip(Radius.md).background(c.danger).clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Cursus.type.body, color = Color.White, fontWeight = FontWeight.Medium)
    }
}

/** Icon-only tool: 20px glyph in a 40px touch hit (mobile ≥ 40). */
@Composable
fun IconTool(
    icon: Int,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tint: Color = Cursus.colors.textSecondary,
    enabled: Boolean = true,
) {
    Box(
        modifier.size(40.dp).clip(Radius.pill).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = if (enabled) tint else Cursus.colors.textMuted)
    }
}

/** Accent action button: rounded square radius 12, like the nav destinations. */
@Composable
fun AccentSquare(icon: Int, description: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = Cursus.colors
    Box(
        modifier
            .size(44.dp)
            .clip(Radius.lg)
            .background(if (enabled) c.accent else c.disabledFill)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = if (enabled) c.accentOn else c.textMuted)
    }
}

/** 32px `accent.soft` disc with an accent glyph (danger: red-tinted). */
@Composable
fun IconDisc(icon: Int, size: Dp = 32.dp, danger: Boolean = false) {
    val c = Cursus.colors
    val tint = if (danger) c.danger else c.accent
    Box(
        Modifier.size(size).clip(Radius.pill).background(if (danger) c.danger.copy(alpha = 0.14f) else c.accentSoft),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), null, Modifier.size(size * 0.55f), tint = tint)
    }
}

/** Ghost action tile: icon disc + label, no fill until pressed. */
@Composable
fun ActionTile(icon: Int, text: String, onClick: () -> Unit, danger: Boolean = false, modifier: Modifier = Modifier) {
    val c = Cursus.colors
    Row(
        modifier.fillMaxWidth().clip(Radius.lg).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconDisc(icon, danger = danger)
        Text(text, style = Cursus.type.body, color = if (danger) c.danger else c.textPrimary)
    }
}

/** Card: `bg.elevated`, 1px border, radius 12, 10px padding. */
@Composable
fun CursusCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val c = Cursus.colors
    Column(
        modifier
            .fillMaxWidth()
            .clip(Radius.lg)
            .background(c.bgElevated)
            .border(1.dp, c.border, Radius.lg)
            .padding(10.dp),
        content = content,
    )
}

/** Card title: 36px icon disc, title and one-line explanation. */
@Composable
fun CardTitle(icon: Int, title: String, subtitle: String? = null) {
    val c = Cursus.colors
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        IconDisc(icon, size = 36.dp)
        Column {
            Text(title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.Medium)
            if (subtitle != null) Text(subtitle, style = Cursus.type.meta, color = c.textSecondary)
        }
    }
}

/** Navigation row: icon disc · title / subtitle · chevron. */
@Composable
fun NavRow(icon: Int, title: String, subtitle: String? = null, onClick: () -> Unit) {
    val c = Cursus.colors
    Row(
        Modifier.fillMaxWidth().clip(Radius.lg).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        IconDisc(icon)
        Column(Modifier.weight(1f)) {
            Text(title, style = Cursus.type.body, color = c.textPrimary)
            if (subtitle != null) Text(subtitle, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Icon(painterResource(R.drawable.ic_chevron_down), null, Modifier.size(16.dp).rotate(-90f), tint = c.textMuted)
    }
}

/** Preference row: label (fills) … control. */
@Composable
fun SwitchRow(title: String, checked: Boolean, onChange: (Boolean) -> Unit, note: String? = null) {
    val c = Cursus.colors
    Row(
        Modifier.fillMaxWidth().clip(Radius.lg).clickable { onChange(!checked) }.padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = Cursus.type.body, color = c.textPrimary)
            if (note != null) Text(note, style = Cursus.type.meta, color = c.textSecondary)
        }
        Spacer(Modifier.width(10.dp))
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(
                checkedTrackColor = c.accent,
                checkedThumbColor = c.accentOn,
                uncheckedTrackColor = c.bgSidebar,
                uncheckedBorderColor = c.border,
                uncheckedThumbColor = c.textMuted,
            ),
        )
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(text, style = Cursus.type.meta, color = Cursus.colors.textMuted, modifier = modifier.padding(start = 8.dp, top = 14.dp, bottom = 4.dp))
}

/** A shareable link: the whole box copies. */
@Composable
fun LinkBox(link: String, onCopied: () -> Unit) {
    val c = Cursus.colors
    val clipboard = LocalClipboardManager.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(Radius.lg)
            .background(c.accentSoft)
            .border(1.dp, c.accent, Radius.lg)
            .clickable {
                clipboard.setText(AnnotatedString(link))
                onCopied()
            }
            .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(link, style = Cursus.type.meta, color = c.accent, modifier = Modifier.weight(1f), maxLines = 4, overflow = TextOverflow.Ellipsis)
        Icon(painterResource(R.drawable.ic_copy), "Copy", Modifier.size(20.dp), tint = c.accent)
    }
}

/** Standard field: radius 10, `bg.sidebar` fill, 1px border → accent when focused. */
@Composable
fun CursusField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    password: Boolean = false,
    singleLine: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
) {
    val c = Cursus.colors
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth(),
        placeholder = { Text(placeholder, color = c.textSecondary, style = Cursus.type.body) },
        singleLine = singleLine,
        textStyle = Cursus.type.body.copy(color = c.textPrimary),
        shape = Radius.md,
        visualTransformation = if (password) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
        keyboardOptions = if (password) KeyboardOptions(keyboardType = KeyboardType.Password) else KeyboardOptions.Default,
        trailingIcon = trailing,
        colors = TextFieldDefaults.colors(
            focusedContainerColor = c.bgSidebar,
            unfocusedContainerColor = c.bgSidebar,
            disabledContainerColor = c.bgSidebar,
            focusedIndicatorColor = Color.Transparent,
            unfocusedIndicatorColor = Color.Transparent,
            cursorColor = c.accent,
        ),
    )
}

/** Avatar: profile picture (data URL) or initials on `accent.soft`. */
@Composable
fun Avatar(name: String, image: String?, size: Dp, modifier: Modifier = Modifier, group: Boolean = false) {
    val c = Cursus.colors
    val bitmap = remember(image) { image?.let(::decodeDataUrl) }
    Box(modifier.size(size).clip(Radius.pill).background(c.accentSoft), contentAlignment = Alignment.Center) {
        if (bitmap != null) {
            Image(bitmap, name, Modifier.size(size), contentScale = ContentScale.Crop)
        } else if (group) {
            Icon(painterResource(R.drawable.ic_user_group), null, Modifier.size(size * 0.5f), tint = c.accent)
        } else {
            Text(
                initials(name),
                color = c.accent,
                style = if (size >= 56.dp) Cursus.type.title else Cursus.type.body,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

fun initials(name: String): String =
    name.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }.take(2)
        .joinToString("") { it.substring(0, it.offsetByCodePoints(0, 1)).uppercase() }
        .ifEmpty { "?" }

fun decodeDataUrl(url: String): ImageBitmap? = runCatching {
    val data = Base64.decode(url.substringAfter("base64,"), Base64.DEFAULT)
    BitmapFactory.decodeByteArray(data, 0, data.size)?.asImageBitmap()
}.getOrNull()

/** Unread pill: accent on accent.soft. */
@Composable
fun UnreadBadge(count: Long, muted: Boolean = false) {
    val c = Cursus.colors
    Box(
        Modifier.widthIn(min = 22.dp).clip(Radius.pill).background(if (muted) c.bgSidebar else c.accentSoft)
            .padding(horizontal = 7.dp, vertical = 2.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            if (count > 999) "999+" else if (count > 0) count.toString() else "•",
            style = Cursus.type.meta,
            color = if (muted) c.textSecondary else c.accent,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

/** Page column capped at 760dp, 16dp sides, 14dp between cards. */
@Composable
fun PageColumn(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 760.dp).fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
            content = content,
        )
    }
}
