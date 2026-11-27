package chat.cursus.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class StampsTest {
    @Test
    fun epochTimesAreNotRealDays() {
        // The chat banner and some events carry the Unix epoch.
        assertNull(Stamps.parse("1970-01-01T00:00:00Z"))
        assertNull(Stamps.day("1970-01-01T00:00:00Z"))
        assertEquals("", Stamps.time("1970-01-01T00:00:00Z"))
        assertEquals("", Stamps.list("1970-01-01T00:00:00Z"))
    }

    @Test
    fun realTimesParse() {
        assertNotNull(Stamps.parse("2026-11-27T17:30:00.123456789Z"))
        assertNotNull(Stamps.day("2026-11-27T17:30:00Z"))
    }
}
