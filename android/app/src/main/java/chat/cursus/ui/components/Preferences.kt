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
import androidx.compose.foundation.shape.RoundedCornerShape
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
import chat.cursus.ui.theme.Cursus

private val PreferenceShape = RoundedCornerShape(20.dp)

/** A quiet inset surface; headings stay outside so the rows can breathe. */
@Composable
fun PreferenceGroup(title: String? = null, note: String? = null, content: @Composable ColumnScope.() -> Unit) {
    val c = Cursus.colors
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(9.dp)) {
        if (title != null) Text(
            title, style = Cursus.type.meta, fontWeight = FontWeight.SemiBold,
            color = c.textSecondary, modifier = Modifier.padding(horizontal = 16.dp),
        )
        Column(
            Modifier.fillMaxWidth().clip(PreferenceShape).background(c.bgElevated)
                .border(1.dp, c.border.copy(alpha = 0.5f), PreferenceShape).padding(vertical = 4.dp),
            content = content,
        )
        if (note != null) Text(note, style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.padding(horizontal = 16.dp))
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
    control: @Composable () -> Unit,
) {
    val c = Cursus.colors
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        // At small window widths, decoration must yield space to the label.
        val showIcon = icon != null && maxWidth >= 360.dp && androidx.compose.ui.platform.LocalDensity.current.fontScale <= 1.3f
        val inset = if (maxWidth < 360.dp) 12.dp else 16.dp
        Column(
            Modifier.fillMaxWidth().defaultMinSize(minHeight = 68.dp)
                .then(if (checked != null && onToggle != null) Modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onToggle) else Modifier)
                .padding(horizontal = inset, vertical = if (subtitle == null) 8.dp else 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                if (showIcon) Box(
                    Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)).background(c.accentSoft),
                    contentAlignment = Alignment.Center,
                ) { Icon(painterResource(requireNotNull(icon)), null, Modifier.size(18.dp), tint = if (enabled) c.accent else c.textMuted) }
                Text(label, style = Cursus.type.body, fontWeight = FontWeight.Medium,
                    color = if (enabled) c.textPrimary else c.textSecondary, modifier = Modifier.weight(1f))
                control()
            }
            if (!subtitle.isNullOrBlank()) Row(
                Modifier.fillMaxWidth().padding(start = if (showIcon) 44.dp else 0.dp)
                    .then(if (onInfo != null) Modifier.clickable(onClick = onInfo) else Modifier),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(subtitle, style = Cursus.type.meta, color = c.textSecondary, modifier = Modifier.weight(1f))
                if (onInfo != null) Icon(painterResource(chat.cursus.R.drawable.ic_info_circle), "About $label",
                    Modifier.size(18.dp), tint = c.textMuted)
            }
        }
    }
}
