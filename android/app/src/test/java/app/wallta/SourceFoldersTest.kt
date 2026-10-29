package app.wallta

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SourceFoldersTest {
    private fun source() = JSONObject("""{"kind":"webdav","location":"https://nas.example/a","recursive":true}""")
    @Test fun secondFolderChangesFingerprint() {
        val source = source()
        val before = MediaSafety.sourceConfig(source)
        source.put("folders", JSONArray().put(JSONObject().put("location", source.getString("location"))).put(JSONObject().put("location", "https://nas.example/b")))
        assertNotEquals(before, MediaSafety.sourceConfig(source))
    }
    @Test fun legacyAndEmptyFoldersKeepSingleton() {
        val source = source()
        assertEquals(listOf("https://nas.example/a"), SourceFolders.locations(source))
        source.put("folders", JSONArray())
        assertEquals(listOf("https://nas.example/a"), SourceFolders.locations(source))
    }
    @Test fun authoritativeFoldersMirrorFirstAndIgnoreLabelsInFingerprint() {
        val source = source().put("location", "old").put("folders", JSONArray("""[{"location":"https://nas.example/b","name":"B"},{"location":"https://nas.example/c"}]"""))
        SourceFolders.normalize(source)
        assertEquals("https://nas.example/b", source.getString("location"))
        val before = MediaSafety.sourceConfig(source)
        source.getJSONArray("folders").getJSONObject(0).put("name", "Renamed")
        assertEquals(before, MediaSafety.sourceConfig(source))
        assertTrue(SourceFolders.contains(source, "https://nas.example/c/image.jpg"))
        assertFalse(SourceFolders.contains(source, "https://nas.example/outside/image.jpg"))
    }
    @Test fun rejectsEmptyDuplicateOverLimitAndNonFolderProviders() {
        listOf("""[{"location":""}]""", """[{"location":"https://nas.example/a"},{"location":"https://nas.example/a/"}]""").forEach { folders ->
            assertThrows(IllegalArgumentException::class.java) { SourceFolders.normalize(source().put("folders", JSONArray(folders))) }
        }
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.locations(source().put("folders", JSONArray().apply { repeat(51) { put(JSONObject().put("location", "https://nas.example/$it")) } })) }
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.locations(source().put("kind", "photos").put("folders", JSONArray("""[{"location":"album"}]"""))) }
    }
    @Test fun relayCannotExpandOrChangeApprovedRoot() {
        val session = JSONObject("""{"phase":"ready","folder":"approved"}""")
        val source = JSONObject("""{"kind":"drive","location":"https://drive.google.com/drive/folders/approved"}""")
        SourceFolders.relayScope(source, session)
        source.put("folders", JSONArray("""[{"location":"approved"},{"location":"other"}]"""))
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.relayScope(source, session) }
        source.put("folders", JSONArray()).put("location", "other")
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.relayScope(source, session) }
    }
    @Test fun webdavFoldersMustShareOneCredentialOrigin() {
        val source = source().put("folders", JSONArray("""[{"location":"https://nas.example/a"},{"location":"https://other.example/b"}]"""))
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.normalize(source) }
    }
    @Test fun overlappingSafTreesUseOneDocumentIdentity() {
        assertEquals(SourceFolders.documentKey("content://docs/tree/parent/document/parent%2Fchild%2Fimage.jpg"), SourceFolders.documentKey("content://docs/tree/parent%2Fchild/document/parent%2Fchild%2Fimage.jpg"))
        assertNotEquals(SourceFolders.documentKey("content://docs/document/a"), SourceFolders.documentKey("content://other/document/a"))
    }
    @Test fun disabledRelayConnectionCanForgetAndReselectItsApprovedFolder() {
        val session = JSONObject("""{"phase":"ready","folder":"approved"}""")
        val source = JSONObject("""{"kind":"drive","location":"","enabled":false,"folders":[]}""")
        SourceFolders.normalize(source); SourceFolders.relayScope(source, session)
        source.put("location", "approved").put("enabled", true)
        SourceFolders.relayScope(source, session)
        source.put("location", "other")
        assertThrows(IllegalArgumentException::class.java) { SourceFolders.relayScope(source, session) }
    }
}
