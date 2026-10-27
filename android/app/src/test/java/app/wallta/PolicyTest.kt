package app.wallta

import org.junit.Assert.*
import org.junit.Test
import java.time.LocalDateTime

class PolicyTest {
    @Test fun overnightUsesPreviousWeekdayAfterMidnight() {
        assertTrue(Policy.inSchedule(true, setOf(1), "22:00", "06:00", LocalDateTime.parse("2026-10-27T02:00:00")))
        assertFalse(Policy.inSchedule(true, setOf(2), "22:00", "06:00", LocalDateTime.parse("2026-10-27T02:00:00")))
    }
    @Test fun sameTimeMeansAllDayAndEndIsExclusive() {
        assertTrue(Policy.inSchedule(true, setOf(2), "07:00", "07:00", LocalDateTime.parse("2026-10-27T12:00:00")))
        assertFalse(Policy.inSchedule(true, setOf(2), "07:00", "23:00", LocalDateTime.parse("2026-10-27T23:00:00")))
    }
    @Test fun shuffleConsumesWholeCycleAcrossRestarts() {
        val ids = listOf("a", "b", "c")
        val used = mutableSetOf<String>()
        repeat(3) { used += Policy.next(ids, used, true, null)!! }
        assertEquals(ids.toSet(), used)
        assertNotEquals("c", Policy.next(ids, used, true, "c"))
    }
    @Test fun sequentialWrapsAndRemovedCurrentStartsFirst() {
        assertEquals("a", Policy.next(listOf("a", "b"), emptySet(), false, "b"))
        assertEquals("a", Policy.next(listOf("a", "b"), emptySet(), false, "gone"))
    }
    @Test fun manifestResolvesRelativeImagesAndRejectsUnsafeSchemes() {
        val items = Policy.manifest("""{"images":["a.jpg",{"url":"https://cdn.example/b.png","name":"B"},"file:///etc/passwd","javascript:alert(1)"]}""", "https://example.com/gallery/list.json", "s")
        assertEquals(2, items.size)
        assertEquals("https://example.com/gallery/a.jpg", items[0].url)
        assertEquals("B", items[1].name)
    }
    @Test fun manifestRejectsTooManyItems() {
        val json = org.json.JSONArray()
        repeat(10001) { json.put("https://example.com/$it.jpg") }
        assertThrows(IllegalArgumentException::class.java) { Policy.manifest(json.toString(), "https://example.com/list.json", "s") }
    }
}
