package app.wallta

import org.junit.Assert.*
import org.junit.Test

class MediaSafetyTest {
    @Test fun visualDimensionsBoundBothOrientationsWithoutUpscaling() {
        assertEquals(2048 to 1536, MediaSafety.visualSize(12000, 9000))
        assertEquals(1536 to 2048, MediaSafety.visualSize(9000, 12000))
        assertEquals(640 to 480, MediaSafety.visualSize(640, 480))
        assertEquals(2048 to 1, MediaSafety.visualSize(120000, 1))
    }
    @Test fun sampleSizeBoundsDecodeBeforeResizing() {
        val sample = MediaSafety.sampleSize(12000, 9000)
        assertTrue(12000 / sample <= 4096)
        assertTrue(9000 / sample <= 4096)
        assertTrue(12000L / sample * (9000 / sample) <= 4_194_304)
    }
    @Test fun credentialsStayInsideExactOriginAndPath() {
        val base = "https://relay.example/wallta/api/drive/media/"
        assertTrue(MediaSafety.within("https://relay.example:443/wallta/api/drive/media/abc?x=y", base))
        listOf("https://other.example/wallta/api/drive/media/abc", "https://relay.example:8443/wallta/api/drive/media/abc", "http://relay.example/wallta/api/drive/media/abc", "https://relay.example/wallta/api/drive/media-evil/abc", "https://relay.example/wallta/api/drive/media/%2e%2e/secret", "https://u:p@relay.example/wallta/api/drive/media/abc").forEach { assertFalse(it, MediaSafety.within(it, base)) }
    }
    @Test fun sequentialFailureSkipsBadCandidateWithoutPersistingIt() {
        val attempted = mutableListOf<String>()
        val result = MediaSafety.firstUsable(listOf("bad", "good"), emptySet(), false, null) { id -> attempted += id; if (id == "bad") error("corrupt image"); "$id.jpg" }
        assertEquals("good" to "good.jpg", result)
        assertEquals(listOf("bad", "good"), attempted)
    }
    @Test fun attemptsAreBoundedAndNeverRepeatFailedItem() {
        val attempted = mutableListOf<String>()
        assertThrows(IllegalStateException::class.java) { MediaSafety.firstUsable((0..100).map { "$it" }, emptySet(), false, null) { id -> attempted += id; error("unavailable") } }
        assertEquals(20, attempted.size)
        assertEquals(20, attempted.toSet().size)
    }
    @Test fun sequentialFailureContinuesAfterCurrentInsteadOfRestartingCatalog() {
        val attempted = mutableListOf<String>()
        val result = MediaSafety.firstUsable(listOf("a", "b", "c"), emptySet(), false, "a") { id -> attempted += id; if (id == "b") error("missing"); id }
        assertEquals("c", result.first)
        assertEquals(listOf("b", "c"), attempted)
    }
    @Test fun cancellationGuardStopsBeforeTryingNextCandidate() {
        var attempts = 0
        assertThrows(InterruptedException::class.java) { MediaSafety.firstUsable(listOf("a", "b"), emptySet(), false, null, ensureCurrent = { if (attempts > 0) throw InterruptedException("paused") }) { attempts++; error("bad") } }
        assertEquals(1, attempts)
    }
    @Test fun cancellationAfterLoadDisposesResourceWithoutTryingAnotherCandidate() {
        var loaded = false; var disposed = false
        assertThrows(InterruptedException::class.java) {
            MediaSafety.firstUsable(listOf("a", "b"), emptySet(), false, null, ensureCurrent = { if (loaded) throw InterruptedException("paused") }, discard = { _: String -> disposed = true }) { loaded = true; "bitmap" }
        }
        assertTrue(disposed)
    }
}
