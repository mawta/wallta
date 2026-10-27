package app.wallta

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SourceConfigTest {
    private fun source() = JSONObject("""{"id":"s","name":"NAS","kind":"webdav","location":"https://nas.example/photos","recursive":true,"enabled":true,"username":"alice","password":"old"}""")
    @Test fun recursionAndCredentialsChangeMediaIdentity() {
        val source = source(); val identity = MediaSafety.sourceConfig(source)
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("recursive", false)))
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("password", "new")))
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("username", "bob")))
    }
    @Test fun disablingOrMovingProviderChangesMediaIdentity() {
        val source = source(); val identity = MediaSafety.sourceConfig(source)
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("enabled", false)))
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("location", "https://nas.example/other")))
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("kind", "manifest")))
    }
    @Test fun driveKeyAndManualTokenChangeMediaIdentity() {
        val source = source(); val identity = MediaSafety.sourceConfig(source)
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("apiKey", "key")))
        assertNotEquals(identity, MediaSafety.sourceConfig(JSONObject(source.toString()).put("accessToken", "token")))
    }
    @Test fun labelAndPropertyOrderDoNotChangeProviderIdentity() {
        val source = source()
        assertEquals(MediaSafety.sourceConfig(source), MediaSafety.sourceConfig(JSONObject(source.toString()).put("name", "Renamed")))
        val reordered = JSONObject("""{"password":"old","username":"alice","enabled":true,"recursive":true,"location":"https://nas.example/photos","kind":"webdav"}""")
        assertEquals(MediaSafety.sourceConfig(source), MediaSafety.sourceConfig(reordered))
    }
    @Test fun absentAndExplicitEmptyCredentialsHaveSameEffectiveIdentity() {
        val source = source().apply { remove("password") }
        assertEquals(MediaSafety.sourceConfig(source), MediaSafety.sourceConfig(JSONObject(source.toString()).put("password", "")))
    }
}
