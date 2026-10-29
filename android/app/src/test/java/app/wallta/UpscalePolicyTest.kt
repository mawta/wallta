package app.wallta

import org.junit.Assert.*
import org.junit.Test

class UpscalePolicyTest {
    @Test fun processingDecodePreserves4kAndPortraitFullHdPixels() {
        assertEquals(1, UpscalePolicy.sampleSize(3840, 2160))
        assertEquals(1, UpscalePolicy.sampleSize(1080, 1920))
        assertEquals(2, UpscalePolicy.sampleSize(8000, 6000))
    }
    @Test fun bounds4kPreserveAspectInBothOrientations() {
        assertEquals(3840 to 2160, UpscalePolicy.target(1920, 1080))
        assertEquals(2160 to 3840, UpscalePolicy.target(1080, 1920))
        assertEquals(2160 to 2160, UpscalePolicy.target(100, 100))
        assertEquals(3840 to 960, UpscalePolicy.target(8000, 2000))
    }
    @Test fun rejectsInvalidDimensionsModesAndSelections() {
        assertThrows(IllegalArgumentException::class.java) { UpscalePolicy.target(0, 2) }
        assertThrows(IllegalArgumentException::class.java) { UpscalePolicy.target(12001, 10000) }
        listOf(emptyList(), listOf("x"), List(51) { Policy.hash("$it") }, List(2) { Policy.hash("same") }).forEach { ids ->
            assertThrows(IllegalArgumentException::class.java) { UpscalePolicy.selection(ids, "fast") }
        }
        assertThrows(IllegalArgumentException::class.java) { UpscalePolicy.selection(listOf(Policy.hash("x")), "other") }
        assertThrows(IllegalArgumentException::class.java) { UpscalePolicy.selection(listOf(Policy.hash("x")), "ai") }
    }
}
