package chat.cursus.ui.components

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.foundation.interaction.collectIsHoveredAsState
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
    GroupShortDescr("Short description", "Shown under the name.", "Short description", multiline = true),
    GroupWelcome("Welcome message", "Sent to every new member when they join.", "Welcome to the group! Please…", multiline = true),
}

enum class LineStyle { Name, Sub, Body }

/** Plain text with a pencil; tapping opens the editor sheet (Linux `editable_line`). */
@Composable
fun EditableLine(value: String, emptyText: String, style: LineStyle, rich: Boolean = style != LineStyle.Name, onEdit: () -> Unit) {
    val c = Cursus.colors
    val textStyle = when (style) {
        LineStyle.Name -> Cursus.type.title
        LineStyle.Sub -> Cursus.type.body
        LineStyle.Body -> Cursus.type.body
    }
    // Header lines (name, bio) are centred under the picture; paragraphs
    // start at the row inset under their section label.
    val centred = style != LineStyle.Body
    Row(
        Modifier.fillMaxWidth().clip(RowShape).clickable(onClick = onEdit).padding(horizontal = 12.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = if (centred) Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally) else Arrangement.spacedBy(6.dp),
    ) {
        ProfileText(
            value.ifBlank { emptyText }, rich = rich,
            style = if (style == LineStyle.Name) textStyle.copy(fontSize = textStyle.fontSize * 1.25f) else textStyle,
            color = when {
                value.isBlank() -> c.textMuted
                style == LineStyle.Sub -> c.textSecondary
                else -> c.textPrimary
            },
            fontWeight = if (style == LineStyle.Name && value.isNotBlank()) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = if (style == LineStyle.Body) 4 else 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = if (centred) androidx.compose.ui.text.style.TextAlign.Center else null,
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
    val literalName = field in setOf(EditField.OwnName, EditField.GroupName, EditField.GroupAlias, EditField.ContactAlias)
    CursusOverlay(onDismiss) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
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
                    .heightIn(min = if (field.multiline) 140.dp else 0.dp, max = 240.dp).padding(12.dp),
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
            if (field.required) Text("Names are plain text and must start with a letter, number or emoji.", style = Cursus.type.meta, color = c.textSecondary)
            Text(if (literalName) "Names are displayed exactly as typed." else "Format: **bold** · *italic* · _italic_ · ~strike~ · `code`", style = Cursus.type.meta, color = c.textSecondary)
            if (value.isNotBlank()) {
                Text("Preview", style = Cursus.type.meta, color = c.textMuted)
                ProfileText(value, rich = !literalName, style = Cursus.type.body, color = c.textPrimary, maxLines = 6, overflow = TextOverflow.Ellipsis)
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
}

/** Linux `profile_photo_actions`: set / remove picture, 32px secondary buttons. */
class PhotoActions(val hasImage: Boolean, val onPick: () -> Unit, val onClear: () -> Unit)

/** Linux `profile_identity`: 84dp picture left, name lines right. When the
 *  picture can be changed, tapping it opens a Change / Remove sheet. */
@Composable
fun ProfileIdentity(name: String, image: String?, group: Boolean = false, photo: PhotoActions? = null, lines: @Composable () -> Unit) {
    val c = Cursus.colors
    var sheet by remember { mutableStateOf(false) }
    // Centred hero: picture, then the name and the lines under it.
    Column(
        Modifier.fillMaxWidth().padding(top = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(if (photo != null) Modifier.clip(Radius.pill).clickable { sheet = true } else Modifier) {
            Avatar(name, image, 96.dp, group = group)
        }
        Box(Modifier.size(4.dp))
        lines()
    }
    if (sheet && photo != null) {
        CursusOverlay({ sheet = false }) {
            Text("Profile picture", style = Cursus.type.body, color = c.textPrimary, fontWeight = FontWeight.SemiBold)
            Column {
                SheetItem(R.drawable.ic_image_pen, "Change picture", c.textPrimary, c.accent) { sheet = false; photo.onPick() }
                if (photo.hasImage) {
                    SheetItem(R.drawable.ic_trash_delete, "Remove picture", c.danger, c.danger) { sheet = false; photo.onClear() }
                }
                SheetItem(R.drawable.ic_cancel, "Cancel", c.textPrimary, c.textSecondary) { sheet = false }
            }
        }
    }
}

@Composable
private fun SheetItem(icon: Int, label: String, color: androidx.compose.ui.graphics.Color, tint: androidx.compose.ui.graphics.Color, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Radius.lg).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(painterResource(icon), null, Modifier.size(20.dp), tint = tint)
        Text(label, style = Cursus.type.body, color = color)
    }
}

/** Read-only name of a profile header: large, centred. */
@Composable
fun ProfileName(name: String) {
    val title = Cursus.type.title
    ProfileText(
        name, rich = false, style = title.copy(fontSize = title.fontSize * 1.25f), color = Cursus.colors.textPrimary, fontWeight = FontWeight.SemiBold,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp), maxLines = 2, overflow = TextOverflow.Ellipsis,
    )
}

/** A centred secondary line of a profile header (bio, short description). */
@Composable
fun HeroLine(text: String) {
    ProfileText(
        text, style = Cursus.type.body, color = Cursus.colors.textSecondary,
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
    )
}

/** One of the actions under a profile's name (Linux `quick_actions`). */
class QuickAction(val icon: Int, val label: String, val active: Boolean = false, val onClick: () -> Unit)

/** One complete action surface: icon and label share its hit target. */
@Composable
fun QuickActions(vararg actions: QuickAction) {
    val c = Cursus.colors
    Box(Modifier.fillMaxWidth().padding(horizontal = 8.dp), contentAlignment = Alignment.Center) {
        Row(Modifier.widthIn(max = (92 * actions.size).dp).fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (a in actions) {
                val interactions = androidx.compose.runtime.remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
                val hovered by interactions.collectIsHoveredAsState()
                val tint = if (a.active || hovered) c.accent else c.textSecondary
                Column(
                    Modifier.weight(1f).clip(RoundedCornerShape(16.dp))
                        .background(if (hovered) c.accentSoft else c.bgElevated)
                        .border(1.dp, if (hovered) c.accent else c.border, RoundedCornerShape(16.dp))
                        .clickable(interactions, indication = androidx.compose.foundation.LocalIndication.current, onClick = a.onClick).padding(horizontal = 4.dp, vertical = 12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(painterResource(a.icon), null, Modifier.size(24.dp), tint = tint)
                    Text(a.label, style = Cursus.type.meta, color = c.textSecondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** A paragraph of a profile under its section label (Linux `about_block`). */
@Composable
fun AboutBlock(label: String, text: String) {
    Column {
        CardTitle(0, label)
        ProfileText(text, style = Cursus.type.body, color = Cursus.colors.textPrimary, modifier = Modifier.padding(horizontal = 12.dp, vertical = 2.dp))
    }
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
    PreferenceRow(label, icon = icon, control = control)
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
