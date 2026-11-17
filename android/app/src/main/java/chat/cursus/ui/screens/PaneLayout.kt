package chat.cursus.ui.screens

/** Android has two shells: a narrow push stack, or list + detail with a left dock. */
internal object PaneLayout {
    fun isSplit(widthDp: Float): Boolean = widthDp >= 600f
    fun listWidth(widthDp: Float): Float = (widthDp * 0.34f).coerceIn(260f, 340f)
}
