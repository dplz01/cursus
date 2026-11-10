package chat.cursus.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.cursus.ThemeMode

/**
 * Cursus tokens — names and values from docs/DESIGN_SYSTEM.md § Color
 * (source of truth: linux/src/theme.rs). Keep them identical so reviews can
 * diff this file against the table.
 */
@Immutable
data class CursusColors(
    val bgApp: Color,
    val bgSidebar: Color,
    val bgList: Color,
    val bgPanel: Color,
    val bgElevated: Color,
    val bgHover: Color,
    val textPrimary: Color,
    val textSecondary: Color,
    val textMuted: Color,
    val accent: Color,
    val accentHover: Color,
    val accentPress: Color,
    val accentOn: Color,
    val accentSoft: Color,
    val accentSoftPress: Color,
    val border: Color,
    val danger: Color,
    val success: Color,
    val gold: Color,
    /** Bright yellow for things that need attention (pending-links button). */
    val attention: Color,
    /** Unplayed waveform bars (soft accent in light, calmer than grey);
     *  `voiceRestOut` on the tinted outgoing bubble. */
    val voiceRest: Color,
    val voiceRestOut: Color,
    val disabledFill: Color,
    /** Text fields, composer pill and quiet buttons (Linux `bg_field`). */
    val bgField: Color,
    /** Message bubbles; meta (time) and delivered tick inside sent ones. */
    val bubbleIn: Color,
    val bubbleOut: Color,
    val bubbleOutMeta: Color,
    val bubbleOutTick: Color,
    val isDark: Boolean,
)

private val Accent = Color(0xFF2F6FED)
private val AccentDark = Color(0xFF5B8CFF)

val LightTokens = CursusColors(
    // Sidebar, list and bars are white; the conversation / detail panes sit
    // on a soft grey so white bubbles and cards lift off it (linux theme.rs).
    bgApp = Color(0xFFEEF0F4),
    bgSidebar = Color(0xFFFFFFFF),
    bgList = Color(0xFFFFFFFF),
    bgPanel = Color(0xFFEEF0F4),
    bgElevated = Color(0xFFFFFFFF),
    bgHover = Color(0xFFEDEFF3),
    textPrimary = Color(0xFF12141A),
    textSecondary = Color(0xFF5C6370),
    textMuted = Color(0xFF8B919C),
    accent = Accent,
    accentHover = Color(0xFF255DD1),
    accentPress = Color(0xFF1C4CB8),
    accentOn = Color(0xFFFFFFFF),
    accentSoft = Accent.copy(alpha = 0.12f),
    accentSoftPress = Accent.copy(alpha = 0.30f),
    border = Color(0xFFDDE1E7),
    danger = Color(0xFFC62828),
    success = Color(0xFF2E7D32),
    gold = Color(0xFFB58B0F),
    attention = Color(0xFFF5B800),
    voiceRest = Accent.copy(alpha = 0.32f),
    voiceRestOut = Accent.copy(alpha = 0.45f),
    disabledFill = Color(0xFFD1D5DA),
    bgField = Color(0xFFF2F4F7),
    bubbleIn = Color(0xFFFFFFFF),
    bubbleOut = Color(0xFFD8E5FF),
    bubbleOutMeta = Color(0xFF5A6C8F),
    bubbleOutTick = Accent,
    isDark = false,
)

val DarkTokens = CursusColors(
    bgApp = Color(0xFF12141A),
    bgSidebar = Color(0xFF181B22),
    bgList = Color(0xFF1C2028),
    bgPanel = Color(0xFF1C2028),
    bgElevated = Color(0xFF262B36),
    bgHover = Color(0xFF262B36),
    textPrimary = Color(0xFFF2F3F5),
    textSecondary = Color(0xFFA0A6B3),
    textMuted = Color(0xFF6E7585),
    accent = AccentDark,
    accentHover = Color(0xFF6B99FF),
    accentPress = Color(0xFF4D7AEB),
    accentOn = Color(0xFF0B1020),
    accentSoft = AccentDark.copy(alpha = 0.18f),
    accentSoftPress = AccentDark.copy(alpha = 0.40f),
    border = Color(0xFF2E3440),
    danger = Color(0xFFFF6B6B),
    success = Color(0xFF6BCB77),
    gold = Color(0xFFECD04F),
    attention = Color(0xFFF5C518),
    voiceRest = Color(0xFF6E7585),
    voiceRestOut = Color(0xFF7F93BC),
    disabledFill = Color(0xFF404D66),
    bgField = Color(0xFF181B22),
    bubbleIn = Color(0xFF262B36),
    bubbleOut = AccentDark.copy(alpha = 0.18f),
    bubbleOutMeta = Color(0xFF6E7585),
    bubbleOutTick = AccentDark,
    isDark = true,
)

/** Shapes: only these, only through the components (DESIGN_SYSTEM § Spacing). */
object Radius {
    val sm = RoundedCornerShape(8.dp)
    val md = RoundedCornerShape(10.dp)
    val lg = RoundedCornerShape(12.dp)
    val bubble = RoundedCornerShape(16.dp)
    val pill = RoundedCornerShape(percent = 50)
}

/** Mobile column of the typography table: title 17, body 16, meta 13. */
@Immutable
data class CursusType(val title: TextStyle, val body: TextStyle, val meta: TextStyle)

private val MobileType = CursusType(
    title = TextStyle(fontSize = 17.sp, fontWeight = FontWeight.SemiBold),
    body = TextStyle(fontSize = 16.sp),
    meta = TextStyle(fontSize = 13.sp),
)

private val LocalColors = staticCompositionLocalOf { LightTokens }
private val LocalType = staticCompositionLocalOf { MobileType }

object Cursus {
    val colors: CursusColors @Composable get() = LocalColors.current
    val type: CursusType @Composable get() = LocalType.current
}

private fun scheme(t: CursusColors): ColorScheme {
    val base = if (t.isDark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = t.accent,
        onPrimary = t.accentOn,
        primaryContainer = t.accentSoft,
        onPrimaryContainer = t.accent,
        secondaryContainer = t.accentSoft,
        onSecondaryContainer = t.accent,
        background = t.bgApp,
        onBackground = t.textPrimary,
        surface = t.bgPanel,
        onSurface = t.textPrimary,
        surfaceVariant = t.bgSidebar,
        onSurfaceVariant = t.textSecondary,
        surfaceContainer = t.bgSidebar,
        surfaceContainerHigh = t.bgElevated,
        surfaceContainerHighest = t.bgElevated,
        surfaceContainerLow = t.bgList,
        outline = t.border,
        outlineVariant = t.border,
        error = t.danger,
    )
}

@Composable
fun CursusTheme(mode: ThemeMode, content: @Composable () -> Unit) {
    val dark = when (mode) {
        ThemeMode.System -> isSystemInDarkTheme()
        ThemeMode.Light -> false
        ThemeMode.Dark -> true
    }
    val tokens = if (dark) DarkTokens else LightTokens
    CompositionLocalProvider(LocalColors provides tokens, LocalType provides MobileType) {
        MaterialTheme(
            colorScheme = scheme(tokens),
            typography = Typography(
                bodyLarge = MobileType.body,
                bodyMedium = MobileType.body,
                labelLarge = MobileType.meta,
                titleLarge = MobileType.title,
            ),
            content = content,
        )
    }
}
