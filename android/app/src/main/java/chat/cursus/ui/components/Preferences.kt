package chat.cursus.ui.components

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
    control: @Composable () -> Unit,
) {
    val c = Cursus.colors
    Row(
        Modifier.fillMaxWidth().defaultMinSize(minHeight = 68.dp)
            .then(if (checked != null && onToggle != null) Modifier.toggleable(checked, enabled = enabled, role = Role.Switch, onValueChange = onToggle) else Modifier)
            .padding(horizontal = 16.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        if (icon != null) Box(
            Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)).background(c.accentSoft),
            contentAlignment = Alignment.Center,
        ) { Icon(painterResource(icon), null, Modifier.size(18.dp), tint = if (enabled) c.accent else c.textMuted) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(label, style = Cursus.type.body, fontWeight = FontWeight.Medium, color = if (enabled) c.textPrimary else c.textSecondary)
            if (!subtitle.isNullOrBlank()) Text(subtitle, style = Cursus.type.meta, color = c.textSecondary)
        }
        control()
    }
}
