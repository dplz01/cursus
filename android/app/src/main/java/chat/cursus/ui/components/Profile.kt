package chat.cursus.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import chat.cursus.R
import chat.cursus.ui.theme.Cursus
import chat.cursus.ui.theme.Radius

/** Linux `EditField` + `field_editor_meta`: what the editor sheet says. */
enum class EditField(val title: String, val hint: String, val placeholder: String, val multiline: Boolean = false, val required: Boolean = false) {
    OwnName("Display name", "How your contacts see you.", "Your name", required = true),
    OwnBio("Bio", "A short line under your name.", "Bio"),
    OwnDescription("Description", "Shown on your profile.", "Tell people about yourself", multiline = true),
    GroupName("Name", "Every member sees this name.", "Name", required = true),
    GroupAlias("Local name", "Only you see this name.", "Local name"),
    ContactAlias("Local name", "Only you see this name.", "Local name"),
    GroupShortDescr("Short description", "Shown under the name.", "Short description"),
    GroupWelcome("Welcome message", "Sent to every new member when they join.", "Welcome to the group! Please…", multiline = true),
}

enum class LineStyle { Name, Sub, Body }

/** Plain text with a pencil; tapping opens the editor sheet (Linux `editable_line`). */
@Composable
fun EditableLine(value: String, emptyText: String, style: LineStyle, onEdit: () -> Unit) {
    val c = Cursus.colors
    val textStyle = when (style) {
        LineStyle.Name -> Cursus.type.title
        LineStyle.Sub -> Cursus.type.meta
        LineStyle.Body -> Cursus.type.body
    }
    Row(
        Modifier.fillMaxWidth().clip(Radius.lg).clickable(onClick = onEdit).padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            value.ifBlank { emptyText },
            style = textStyle,
            color = when {
                value.isBlank() -> c.textMuted
                style == LineStyle.Sub -> c.textSecondary
                else -> c.textPrimary
            },
            fontWeight = if (style == LineStyle.Name && value.isNotBlank()) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = if (style == LineStyle.Body) 4 else 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Icon(painterResource(R.drawable.ic_edit_3), "Edit", Modifier.size(14.dp), tint = c.textMuted)
    }
}

/** Linux `field_editor_overlay`: title + hint, ✕ / ✓, single or multi-line field. */
@Composable
fun FieldEditor(field: EditField, initial: String, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    val c = Cursus.colors
    var value by rememberSaveable { mutableStateOf(initial) }
    val focus = remember { FocusRequester() }
    val canSave = !field.required || value.isNotBlank()
    CursusOverlay(onDismiss) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                Text(field.title, style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.Bold)
                Text(field.hint, style = Cursus.type.meta, color = c.textSecondary)
            }
            IconTool(R.drawable.ic_cancel, "Cancel", onDismiss)
            Box(
                Modifier.size(36.dp).clip(Radius.pill).background(if (canSave) c.accent else c.disabledFill)
                    .clickable(enabled = canSave) { onSave(value) },
                contentAlignment = Alignment.Center,
            ) { Icon(painterResource(R.drawable.ic_check), "Save", Modifier.size(18.dp), tint = c.accentOn) }
        }
        Box(
            Modifier.fillMaxWidth().clip(Radius.md).background(c.bgField).border(1.dp, c.accent, Radius.md)
                .heightIn(min = if (field.multiline) 140.dp else 0.dp).padding(12.dp),
        ) {
            if (value.isEmpty()) Text(field.placeholder, style = Cursus.type.body, color = c.textSecondary)
            BasicTextField(
                value, { value = it },
                singleLine = !field.multiline,
                textStyle = Cursus.type.body.copy(color = c.textPrimary),
                cursorBrush = SolidColor(c.accent),
                modifier = Modifier.fillMaxWidth().focusRequester(focus),
            )
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/** Linux `profile_photo_actions`: set / remove picture, 32px secondary buttons. */
class PhotoActions(val hasImage: Boolean, val onPick: () -> Unit, val onClear: () -> Unit)

@Composable
private fun PhotoButton(icon: Int, description: String, enabled: Boolean, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(
        Modifier.size(32.dp).clip(Radius.md).background(c.bgField).border(1.dp, c.border, Radius.md)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(18.dp), tint = if (enabled) c.textSecondary else c.textMuted)
    }
}

/** Linux `profile_identity`: 72px avatar (photo buttons under it) left, name lines right. */
@Composable
fun ProfileIdentity(name: String, image: String?, group: Boolean = false, photo: PhotoActions? = null, lines: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Avatar(name, image, 72.dp, group = group)
            if (photo != null) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    PhotoButton(R.drawable.ic_image_pen, "Set picture", true, photo.onPick)
                    PhotoButton(R.drawable.ic_cancel, "Remove picture", photo.hasImage, photo.onClear)
                }
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) { lines() }
    }
}

/** Read-only name, title size, inset to line up with editable lines. */
@Composable
fun ProfileName(name: String) {
    Text(
        name, style = Cursus.type.title, color = Cursus.colors.textPrimary, fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(horizontal = 8.dp), maxLines = 2, overflow = TextOverflow.Ellipsis,
    )
}

/** Linux `choice_chip`: a secondary button showing only the current value. */
@Composable
fun ChoiceChip(value: String, onClick: () -> Unit, enabled: Boolean = true) {
    val c = Cursus.colors
    Box(
        Modifier.clip(Radius.md).background(c.bgField).border(1.dp, c.border, Radius.md)
            .clickable(enabled = enabled, onClick = onClick).padding(horizontal = 10.dp, vertical = 6.dp),
    ) {
        Text(value, style = Cursus.type.meta, color = if (enabled) c.textPrimary else c.textMuted)
    }
}

/** Choice menu: title in muted, options as menu items, the current one in accent. */
@Composable
fun <T> ChoiceMenu(title: String, options: List<Pair<T, String>>, current: T?, onDismiss: () -> Unit, onPick: (T) -> Unit) {
    val c = Cursus.colors
    CursusOverlay(onDismiss) {
        Text(title, style = Cursus.type.meta, color = c.textMuted)
        Column {
            for ((value, label) in options) {
                Text(
                    label,
                    style = Cursus.type.body,
                    color = if (value == current) c.accent else c.textPrimary,
                    fontWeight = if (value == current) FontWeight.SemiBold else FontWeight.Normal,
                    modifier = Modifier.fillMaxWidth().clip(Radius.sm).clickable { onPick(value) }
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                )
            }
        }
    }
}

/** Preference row: [icon] label … control (Linux `pref_row`). */
@Composable
fun PrefRow(label: String, icon: Int? = null, control: @Composable () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (icon != null) Icon(painterResource(icon), null, Modifier.size(20.dp), tint = Cursus.colors.accent)
        Text(label, style = Cursus.type.body, color = Cursus.colors.textPrimary, modifier = Modifier.weight(1f))
        control()
    }
}

/** Header tool (star / mute / settings): no background; accent when on, plain text colour when off. */
@Composable
fun HeaderIcon(icon: Int, description: String, active: Boolean = false, onClick: () -> Unit) {
    val c = Cursus.colors
    Box(
        Modifier.size(40.dp).clip(Radius.pill).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), description, Modifier.size(20.dp), tint = if (active) c.accent else c.textPrimary)
    }
}
