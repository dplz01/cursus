package chat.cursus.ui.components

import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.ui.theme.Cursus


/** A quiet inset surface; headings stay outside so the rows can breathe. */
@Composable
fun PreferenceGroup(title: String? = null, note: String? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (title != null) SectionHeading(title)
        Panel(content = content)
        if (note != null) SectionNote(note)
    }
}

/** One flexible label column and one fixed control column, at every width. */
@Composable
fun PreferenceRow(
    label: String,
    subtitle: String? = null,
    icon: Int? = null,
    checked: Boolean? = null,
    enabled: Boolean = true,
    onToggle: ((Boolean) -> Unit)? = null,
    onInfo: (() -> Unit)? = null,
    onClick: (() -> Unit)? = null,
    control: @Composable () -> Unit,
) {
    val c = Cursus.colors
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        // At small window widths, decoration must yield space to the label.
        val showIcon = icon != null && maxWidth >= 360.dp && androidx.compose.ui.platform.LocalDensity.current.fontScale <= 1.3f
        val inset = 16.dp
        Column(
            Modifier.fillMaxWidth().clip(chat.cursus.ui.theme.Radius.insetControl)
                .defaultMinSize(minHeight = if (subtitle.isNullOrBlank()) 64.dp else 84.dp)
                .then(if (checked != null && onToggle != null) Modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onToggle) else Modifier)
                // A row that opens a choice is one button (Linux `choice_row`).
                .then(if (onClick != null) Modifier.clickable(enabled = enabled, onClick = onClick) else Modifier)
                .padding(horizontal = inset, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                // Bare glyph in the shared 24dp slot, like navigation rows.
                if (showIcon) Box(Modifier.size(RowIconBox), contentAlignment = Alignment.Center) {
                    Icon(painterResource(requireNotNull(icon)), null, Modifier.size(20.dp), tint = if (enabled) c.accent else c.textMuted)
                }
                // The (i) belongs to the name: right after it, its own hit
                // target so it never toggles the row (Linux `info_title`).
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(label, style = Cursus.type.body, fontWeight = FontWeight.Medium,
                        color = if (enabled) c.textPrimary else c.textSecondary, modifier = Modifier.weight(1f, fill = false))
                    if (onInfo != null) Box(
                        Modifier.size(28.dp).clip(chat.cursus.ui.theme.Radius.pill).clickable(onClick = onInfo),
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(painterResource(chat.cursus.R.drawable.ic_info_circle), "About $label", Modifier.size(16.dp), tint = c.textMuted)
                    }
                }
                control()
            }
            if (!subtitle.isNullOrBlank()) Text(
                subtitle, style = Cursus.type.meta, color = c.textSecondary,
                modifier = Modifier.fillMaxWidth().padding(start = if (showIcon) 38.dp else 0.dp),
            )
        }
    }
}
