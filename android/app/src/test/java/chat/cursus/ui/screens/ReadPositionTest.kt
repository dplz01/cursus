package chat.cursus.ui.screens

import chat.cursus.core.Message
import chat.cursus.core.latestReadAnchor
import org.junit.Assert.*
import org.junit.Test

class ReadPositionTest {
    private val messages = listOf(Message(1), Message(2, outgoing = true), Message(3), Message(4, outgoing = true), Message(5))
    @Test fun opensAtLastReadBeforeUnreadMessages() { assertEquals(2L, latestReadAnchor(messages, 2)) }
    @Test fun allReadUsesLatestMessage() { assertEquals(5L, latestReadAnchor(messages, 0)) }
    @Test fun unreadBoundaryBeforeLoadedWindowUsesOldestAvailable() { assertEquals(1L, latestReadAnchor(messages, 50)) }
    @Test fun emptyHistoryHasNoAnchor() { assertNull(latestReadAnchor(emptyList(), 2)) }
}
