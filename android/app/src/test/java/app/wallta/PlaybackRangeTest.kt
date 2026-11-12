package app.wallta

import org.junit.Assert.*
import org.junit.Test

class PlaybackRangeTest {
    @Test fun browserCanReadWholeFileAndSeekWithoutReadingOutsideCacheFile() {
        assertEquals(0L..99L, PlaybackRange.parse(null, 100))
        assertEquals(40L..99L, PlaybackRange.parse("bytes=40-", 100))
        assertEquals(10L..19L, PlaybackRange.parse("bytes=10-19", 100))
        assertEquals(95L..99L, PlaybackRange.parse("bytes=-5", 100))
        assertEquals(80L..99L, PlaybackRange.parse("bytes=80-1000", 100))
        listOf("bytes=100-", "bytes=20-10", "bytes=-", "bytes=0-1,3-4", "bytes=-0", "bytes=999999999999999999999-").forEach { header -> assertThrows(Exception::class.java) { PlaybackRange.parse(header, 100) } }
    }
}
