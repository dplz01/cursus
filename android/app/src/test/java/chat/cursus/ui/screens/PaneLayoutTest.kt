package chat.cursus.ui.screens

import org.junit.Assert.*
import org.junit.Test

class PaneLayoutTest {
    @Test fun narrowWindowsKeepThePushStack() {
        for (width in listOf(280f, 360f, 400f, 599f)) assertFalse(PaneLayout.isSplit(width))
    }
    @Test fun tabletsAlwaysKeepAListAndDetailWithoutARailBreakpoint() {
        for (width in listOf(600f, 800f, 950f, 1280f, 1920f)) {
            assertTrue(PaneLayout.isSplit(width))
            val list = PaneLayout.listWidth(width)
            assertTrue(list in 260f..340f)
            assertTrue(width - list - 1f >= 339f)
        }
    }
}
