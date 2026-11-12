package app.wallta

import org.junit.Assert.*
import org.junit.Test

class MotionPolicyTest {
    @Test fun extensionHintsHandleHiddenDriveAndQueryUrls() {
        assertEquals("video", MotionPolicy.hint("Walk.MP4", "https://www.googleapis.com/drive/v3/files/id?alt=media"))
        assertEquals("gif", MotionPolicy.hint("", "https://example.com/a.gif?token=x"))
        assertEquals("image", MotionPolicy.hint("photo.jpg", "https://example.com/id"))
    }
    @Test fun contentHeadersOverrideNamesAndRejectDisguisedVideo() {
        assertEquals("gif", MotionPolicy.detect("GIF89a".toByteArray()))
        assertEquals("video", MotionPolicy.detect(byteArrayOf(0,0,0,24) + "ftypisom".toByteArray()))
        assertEquals("video", MotionPolicy.detect(byteArrayOf(0x1a, 0x45, 0xdf.toByte(), 0xa3.toByte())))
        assertEquals("image", MotionPolicy.detect("not really mp4".toByteArray()))
    }
    @Test fun playbackStopsWhenSourceRemovedOrHomeSelectionChanges() {
        val source = org.json.JSONObject("""{"id":"home","kind":"url","enabled":true,"location":"https://example.com/a.mp4"}""")
        val state = org.json.JSONObject().put("sources", org.json.JSONArray().put(source))
            .put("settings", org.json.JSONObject().put("homeSourceId", "home").put("animatedWallpaper", true))
        val media = Media("id", "a.mp4", "https://example.com/a.mp4", "home")
        assertTrue(MotionPolicy.available(state, media, false))
        state.getJSONObject("settings").put("homeSourceId", "other")
        assertFalse(MotionPolicy.available(state, media, false))
        assertTrue(MotionPolicy.available(state, media, true))
        source.put("enabled", false)
        assertFalse(MotionPolicy.available(state, media, true))
    }
}
