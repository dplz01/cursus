package chat.cursus.ui.components

import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.requiredSize
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
import androidx.compose.ui.draw.alpha
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
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
            .defaultMinSize(minHeight = 48.dp)
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
            .defaultMinSize(minHeight = 48.dp)
            .clip(Radius.md)
            .background(c.bgField)
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
        modifier.defaultMinSize(minHeight = 48.dp).clip(Radius.md).background(c.danger).clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Cursus.type.body, color = Color.White, fontWeight = FontWeight.Medium)
    }
}

/** Icon-only tool: 20dp glyph in a 48dp touch target. */
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
        modifier.size(48.dp).clip(Radius.control).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = if (enabled) tint else Cursus.colors.textMuted)
    }
}

/** Accent action button: rounded square shared radius, like the nav destinations. */
@Composable
fun AccentSquare(icon: Int, description: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = Cursus.colors
    Box(
        modifier
            .size(48.dp)
            .clip(Radius.smallControl)
            .background(if (enabled) c.accent else c.disabledFill)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = if (enabled) c.accentOn else c.textMuted)
    }
}

/** A 20dp row glyph in the shared 24dp slot. */
@Composable
fun IconDisc(icon: Int, danger: Boolean = false) {
    // The glyph in front of a row: a fixed 24dp box, no backdrop.
    val c = Cursus.colors
    Box(Modifier.size(RowIconBox), contentAlignment = Alignment.Center) {
        Icon(painterResource(icon), null, Modifier.size(20.dp), tint = if (danger) c.danger else c.accent)
    }
}

/** Every row of a page — navigation, preference, action — shares one inset,
 *  icon box, gap and corner, so labels line up down the whole page (the
 *  same numbers as the Linux app's `ROW_*`). */
val RowIconBox = 24.dp
val RowShape = chat.cursus.ui.theme.Radius.control
// Rows live inside panels: their corner is concentric with the panel's.
fun Modifier.pageRow(onClick: (() -> Unit)? = null): Modifier =
    fillMaxWidth().defaultMinSize(minHeight = 64.dp).clip(Radius.insetControl).then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
        .padding(horizontal = 16.dp, vertical = 16.dp)

/** Ghost action tile: icon disc + label, no fill until pressed. */
@Composable
fun ActionTile(icon: Int, text: String, onClick: () -> Unit, modifier: Modifier = Modifier, danger: Boolean = false) {
    val c = Cursus.colors
    Row(
        modifier.pageRow(onClick),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IconDisc(icon, danger = danger)
        Text(text, style = Cursus.type.body, color = if (danger) c.danger else c.textPrimary)
    }
}

/** A flat block of a page (profile hero, paragraphs): no surface. Rows of
 *  settings or actions go in a [Panel]. */
@Composable
fun CursusCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(modifier.fillMaxWidth(), content = content)
}

/** Inset of headings and notes = text inset of rows inside a panel (4 + 16). */
val SectionInset = 20.dp

/** A group of rows (Linux `preference_ui::panel`): flat on the page,
 *  4dp inset. */
@Composable
fun Panel(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val c = Cursus.colors
    // Flat: no surface, border or shadow; the heading above and the gap
    // between groups do the grouping (Linux `preference_ui::panel`). Rows
    // keep the 4dp inset so their text lines up with headings and notes.
    Column(
        modifier.fillMaxWidth().padding(4.dp),
        content = content,
    )
}

/** Section heading above a panel: meta, semibold, `text.secondary`, at the
 *  row text inset. The same heading everywhere (Linux `preference_ui::heading`). */
@Composable
fun SectionHeading(title: String, modifier: Modifier = Modifier) {
    Text(title, style = Cursus.type.meta, color = Cursus.colors.textSecondary, fontWeight = FontWeight.SemiBold,
        modifier = modifier.padding(horizontal = SectionInset))
}

/** Quiet note beside a panel, at the heading inset. */
@Composable
fun SectionNote(text: String, modifier: Modifier = Modifier) {
    Text(text, style = Cursus.type.meta, color = Cursus.colors.textSecondary, modifier = modifier.padding(horizontal = SectionInset))
}

/** Section heading with an optional note under it. */
@Composable
fun CardTitle(icon: Int, title: String, subtitle: String? = null) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        SectionHeading(title)
        if (!subtitle.isNullOrBlank()) SectionNote(subtitle)
    }
}

/** The current value of a row that opens a choice: quiet text + chevron. */
@Composable
fun RowValue(value: String, enabled: Boolean = true) {
    val c = Cursus.colors
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(value, style = Cursus.type.meta, color = if (enabled) c.textSecondary else c.textMuted)
        Icon(painterResource(R.drawable.ic_chevron_down), null, Modifier.size(16.dp).rotate(-90f), tint = c.textMuted)
    }
}

/** Navigation row: icon disc · title / subtitle · chevron. */
@Composable
fun NavRow(icon: Int, title: String, subtitle: String? = null, badge: Long = 0, richSubtitle: Boolean = false, value: String? = null, onClick: () -> Unit) {
    val c = Cursus.colors
    Row(
        Modifier.pageRow(onClick),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IconDisc(icon)
        Column(Modifier.weight(1f)) {
            Text(title, style = Cursus.type.body, color = c.textPrimary)
            if (!subtitle.isNullOrEmpty()) {
                if (richSubtitle) ProfileText(subtitle, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                else Text(subtitle, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (badge > 0) AttentionCount(badge)
        if (value != null) RowValue(value)
        else Icon(painterResource(R.drawable.ic_chevron_down), null, Modifier.size(16.dp).rotate(-90f), tint = c.textMuted)
    }
}

/** Shared switch: a 46×28 track and a 20dp white thumb, with a quiet off state. */
@Composable
fun CursusSwitch(checked: Boolean, onChange: ((Boolean) -> Unit)?, enabled: Boolean = true) {
    val c = Cursus.colors
    val offset by androidx.compose.animation.core.animateDpAsState(if (checked) 24.dp else 4.dp, label = "switch position")
    val track by androidx.compose.animation.animateColorAsState(if (checked) c.accent else c.textSecondary.copy(alpha = 0.24f), label = "switch fill")
    Box(
        Modifier.size(width = 52.dp, height = 32.dp)
            .then(if (enabled) Modifier else Modifier.alpha(0.45f))
            .clip(Radius.pill).background(track)
            .then(if (onChange != null && enabled) Modifier.toggleable(checked, role = androidx.compose.ui.semantics.Role.Switch, onValueChange = onChange) else Modifier),
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(Modifier.offset(x = offset).size(24.dp).clip(Radius.pill).background(Color.White))
    }
}

/** Preference row: label (fills) … control. */
@Composable
fun SwitchRow(title: String, checked: Boolean, onChange: (Boolean) -> Unit, note: String? = null, icon: Int? = null, enabled: Boolean = true) {
    PreferenceRow(title, note, icon, checked, enabled, onChange) {
        CursusSwitch(checked, null, enabled)
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    SectionHeading(text, modifier.padding(top = 14.dp, bottom = 6.dp))
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
            .border(1.dp, c.border, Radius.lg)
            .clickable {
                clipboard.setText(AnnotatedString(link))
                onCopied()
            }
            .padding(12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(link, style = Cursus.type.meta, color = c.accent, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text("Copy", style = Cursus.type.meta, color = c.accent, fontWeight = FontWeight.Medium)
        Icon(painterResource(R.drawable.ic_copy), null, Modifier.size(18.dp), tint = c.accent)
    }
}

/** Standard field: shared radius, `bg.sidebar` fill, 1px border → accent when focused. */
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
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth().border(1.dp, if (focused) c.accent else c.border, Radius.control),
        interactionSource = interactions,
        placeholder = { Text(placeholder, color = c.textSecondary, style = Cursus.type.body) },
        singleLine = singleLine,
        textStyle = Cursus.type.body.copy(color = c.textPrimary),
        shape = Radius.md,
        visualTransformation = if (password) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
        keyboardOptions = if (password) KeyboardOptions(keyboardType = KeyboardType.Password) else KeyboardOptions.Default,
        trailingIcon = trailing,
        colors = TextFieldDefaults.colors(
            focusedContainerColor = c.bgField,
            unfocusedContainerColor = c.bgField,
            disabledContainerColor = c.bgField,
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
    Box(modifier.then(Modifier.requiredSize(size)).clip(Radius.pill).background(c.accentSoft), contentAlignment = Alignment.Center) {
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

/** Decoded avatars and inline previews by their data URL. Rows of the chat
 *  list and the history are composed again whenever they scroll back into
 *  view; without this every one decoded its picture again on the main
 *  thread, which showed as jank while scrolling. */
private val dataUrlCache = object : android.util.LruCache<String, ImageBitmap>(12 * 1024 * 1024) {
    override fun sizeOf(key: String, value: ImageBitmap) = value.width * value.height * 4
}

fun decodeDataUrl(url: String): ImageBitmap? = dataUrlCache.get(url) ?: runCatching {
    val data = Base64.decode(url.substringAfter("base64,"), Base64.DEFAULT)
    BitmapFactory.decodeByteArray(data, 0, data.size)?.asImageBitmap()
}.getOrNull()?.also { dataUrlCache.put(url, it) }

/** Unread pill: solid accent (muted chats: grey), as on Linux and macOS. */
@Composable
fun UnreadBadge(count: Long, muted: Boolean = false) {
    val c = Cursus.colors
    Box(
        Modifier.widthIn(min = 22.dp).clip(Radius.pill).background(if (muted) c.textMuted else c.accent)
            .padding(horizontal = 7.dp, vertical = 2.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            if (count > 999) "999+" else if (count > 0) count.toString() else "•",
            style = Cursus.type.meta,
            color = if (muted) c.bgList else c.accentOn,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

/** Page column capped at 620dp, 16dp insets, 22dp between sections. */
@Composable
fun PageColumn(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
        Column(
            // Same column on every Cursus app: at most 620 wide, centred.
            Modifier.widthIn(max = 620.dp).fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(22.dp),
            content = content,
        )
    }
}

/** Rounded inline field: shared radius, 6×12 padding, placeholder in `text.secondary`. */
@Composable
fun RoundedField(value: String, onValueChange: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier, singleLine: Boolean = true) {
    val c = Cursus.colors
    Box(
        modifier.clip(Radius.lg).background(c.bgField).border(1.dp, c.border, Radius.lg).padding(horizontal = 12.dp, vertical = 9.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (value.isEmpty()) Text(placeholder, style = Cursus.type.body, color = c.textSecondary, maxLines = 1)
        androidx.compose.foundation.text.BasicTextField(
            value, onValueChange,
            singleLine = singleLine,
            textStyle = Cursus.type.body.copy(color = c.textPrimary),
            cursorBrush = androidx.compose.ui.graphics.SolidColor(c.accent),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}


/** Linux `info_toggle`: [SwitchRow] whose title carries an ⓘ that opens
 *  [InfoSheet] with the explanation (`info` = title to paragraphs). */
@Composable
fun InfoSwitchRow(title: String, checked: Boolean, onChange: (Boolean) -> Unit, info: Pair<String, String>) {
    var open by remember { mutableStateOf(false) }
    val label = when (title) {
        "Message previews in chat list" -> "Message previews"
        "Delivery receipts to contacts" -> "Contact delivery receipts"
        "Delivery receipts in small groups" -> "Group delivery receipts"
        "Download images and voice automatically" -> "Automatic media downloads"
        "Hide message text in notifications" -> "Private notifications"
        else -> title
    }
    PreferenceRow(label, settingSummary(title), checked = checked, onToggle = onChange, onInfo = { open = true }) {
        CursusSwitch(checked, null)
    }
    if (open) InfoSheet(info.first, info.second) { open = false }
}

private fun settingSummary(title: String): String? = when (title) {
    "Message previews in chat list" -> "Show the latest message below each name."
    "Blur photos and videos" -> "Reveal media only when you tap it."
    "Block screenshots" -> "Keep chats out of captures and recent apps."
    "Delivery receipts to contacts" -> "Let contacts know their messages arrived."
    "Delivery receipts in small groups" -> "Share delivery status in groups of up to 20."
    "Remove tracking from links" -> "Clean the links you send and open."
    "App lock" -> "Require your password when you return."
    "Download images and voice automatically" -> "Keep small media ready to view and play."
    "Protect IP address" -> "Ask before using unfamiliar file servers."
    "Encrypt stored files" -> "Protect new attachments on this device."
    "Hide message text in notifications" -> "Show the sender, keep the message private."
    else -> null
}

/** Linux `setting_info_overlay`: title with ⓘ, paragraphs, "Got it". */
@Composable
fun InfoSheet(title: String, body: String, onDismiss: () -> Unit) {
    val c = Cursus.colors
    CursusOverlay(onDismiss) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(painterResource(R.drawable.ic_info_circle), null, Modifier.size(20.dp), tint = c.accent)
            Text(title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
        }
        for (para in body.split("\n\n")) Text(para, style = Cursus.type.meta, color = c.textSecondary)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            PrimaryButton("Got it", onDismiss)
        }
    }
}

/** Linux `privacy_confirm_overlay`: a change that shows or shares more asks first. */
@Composable
fun ConfirmSheet(title: String, body: String, action: String, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    val c = Cursus.colors
    CursusOverlay(onDismiss) {
        Text(title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
        Text(body, style = Cursus.type.meta, color = c.textSecondary)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
            SecondaryButton("Cancel", onDismiss)
            PrimaryButton(action, { onConfirm(); onDismiss() })
        }
    }
}


/** Count in the yellow attention style, for list rows (support chats). */
@Composable
fun AttentionCount(count: Long) {
    Box(
        Modifier.defaultMinSize(minWidth = 20.dp, minHeight = 20.dp).clip(Radius.pill)
            .background(Cursus.colors.attention).padding(horizontal = if (count < 10) 0.dp else 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            if (count > 99) "99+" else "$count",
            style = Cursus.type.meta.copy(fontSize = 11.sp, lineHeight = 11.sp),
            color = OnAttention, fontWeight = FontWeight.Bold,
        )
    }
}

/** Icon colour on the attention yellow (dark in both themes). */
val OnAttention = androidx.compose.ui.graphics.Color(0xFF1F1A05)

/** Yellow "needs attention" square with a round count on its top-right
 *  corner (Linux `attention_button`): pending links, unread support chats. */
@Composable
fun AttentionButton(icon: Int, count: Long, description: String, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(Modifier.padding(end = 8.dp).size(36.dp)) {
        Box(
            Modifier.fillMaxSize().clip(Radius.md).background(c.attention).clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            Icon(painterResource(icon), description, Modifier.size(20.dp), tint = OnAttention)
        }
        Box(
            Modifier.align(Alignment.TopEnd).offset(x = 8.dp, y = (-8).dp)
                .defaultMinSize(minWidth = 16.dp, minHeight = 16.dp)
                .clip(Radius.pill).background(c.accent)
                .padding(horizontal = if (count < 10) 0.dp else 4.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                if (count > 99) "99+" else "$count",
                style = Cursus.type.meta.copy(fontSize = 10.sp, lineHeight = 10.sp),
                color = c.accentOn, fontWeight = FontWeight.Bold,
            )
        }
    }
}
