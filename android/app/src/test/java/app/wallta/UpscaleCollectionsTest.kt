package app.wallta

import android.content.Context
import android.content.ContextWrapper
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File
import java.nio.file.Files
import java.security.Provider
import java.security.Security
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE)
class UpscaleCollectionsTest {
    private fun withStore(block: (Context, StateStore, UpscaleCollections) -> Unit) {
        val provider = object : Provider("WalltaUpscaleTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", DriveConnectionCleanupTest.TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        try {
            val context: Context = RuntimeEnvironment.getApplication()
            val store = StateStore(context)
            block(context, store, UpscaleCollections(context, store))
        } finally { Security.removeProvider(provider.name) }
    }
    private fun output(context: Context, source: String? = null): UpscaleManager.Output {
        val folder = File(context.cacheDir.canonicalFile, "fixture").apply { mkdirs() }
        return UpscaleManager.Output(Policy.hash("fixture"), "Original-4K.jpg", 3840, 2160,
            File(folder, "work.img").apply { writeText("result") }, File(folder, "before"), File(folder, "after"), source)
    }
    private fun imported(context: Context, store: StateStore, legacy: Boolean): JSONObject {
        val job = UUID.randomUUID().toString()
        val id = if (legacy) UUID.nameUUIDFromBytes("wallta-4k:$job".toByteArray()).toString() else UUID.randomUUID().toString()
        val location = "wallta-import://${Policy.hash(id)}/$job"
        val folder = PhotoImporter.directory(context, location).apply { mkdirs() }
        val name = "${Policy.hash("file-$job")}.img"; File(folder, name).writeText("original")
        store.put("import:$id", JSONObject().put("phase", "ready").put("location", location).put("items", JSONArray().put(JSONObject().put("file", name).put("name", "Original.png"))))
        val source = JSONObject().put("id", id).put("name", if (legacy) "Ảnh 4K" else "Original photos").put("kind", "local").put("location", location).put("enabled", true)
        val state = store.state(); state.getJSONArray("sources").put(source); store.save(state)
        return source
    }
    @Test fun appendRebasesOwnRevisionIntoOneDisabledCollectionAndExternalSaveStopsLaterAppend() = withStore { context, store, collections ->
        val original = imported(context, store, false)
        val job = UUID.randomUUID().toString(); val output = output(context, original.getString("id")); val initial = store.revision()
        val first = collections.append(job, output, 0, initial)
        val second = collections.append(job, output, 1, first.revision)
        assertTrue(second.revision > first.revision); assertFalse(second.source.getBoolean("enabled"))
        assertEquals(2, store.state().getJSONArray("sources").length())
        assertTrue(second.source.getString("name").contains("Original photos"))
        assertTrue(second.source.getString("name").contains(" • Fast • "))
        assertTrue(second.source.getString("name").matches(Regex(".*[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}$")))
        assertEquals(first.source.getString("name"), second.source.getString("name"))
        val item = collections.storage().getJSONArray("collections").getJSONObject(0)
        assertEquals(2, item.getInt("count")); assertNotNull(java.time.Instant.parse(item.getString("createdAt")))
        store.save(store.state())
        assertThrows(IllegalStateException::class.java) { collections.append(job, output, 2, second.revision) }
        assertEquals(2, collections.storage().getJSONArray("collections").getJSONObject(0).getInt("count"))
    }
    @Test fun multipleSourceRunUsesCapturedAllScopeInsteadOfItsFirstSource() = withStore { context, store, collections ->
        val original = imported(context, store, false)
        val result = collections.append(UUID.randomUUID().toString(), output(context, original.getString("id")).copy(multipleSources = true), 0, store.revision())
        assertTrue(result.source.getString("name").contains("Tất cả nguồn • Fast •"))
        assertFalse(result.source.getString("name").contains("Original photos"))
    }
    @Test fun publishedManifestPersistsOnlyOriginalIdsForComparisonAfterReopening() = withStore { context, store, collections ->
        val original = imported(context, store, false)
        val job = UUID.randomUUID().toString(); val rendered = output(context, original.getString("id"))
        val publication = collections.append(job, rendered, 0, store.revision())
        val published = collections.media(publication.source).single()
        assertEquals(published.id, publication.mediaId); assertEquals(published.sourceId, publication.sourceId)
        assertNotEquals(rendered.id, publication.mediaId)
        val manifest = JSONObject(File(context.filesDir.canonicalFile, "upscaled/$job/collection.json").readText())
        val pairing = manifest.getJSONArray("items").getJSONObject(0).getJSONObject("original")
        assertEquals(rendered.id, pairing.getString("id"))
        assertEquals(original.getString("id"), pairing.getString("sourceId"))
        assertEquals(setOf("id", "sourceId"), pairing.keys().asSequence().toSet())
        assertEquals(1, UpscaleCollections(context, StateStore(context)).storage().getJSONArray("collections").getJSONObject(0).getInt("count"))
    }
    @Test fun originalSnapshotExcludesBothCurrentAndLegacyGeneratedCollections() = withStore { context, store, collections ->
        val original = imported(context, store, false); imported(context, store, true)
        val saved = collections.append(UUID.randomUUID().toString(), output(context), 0, store.revision()).source
        val state = store.state(); val sources = state.getJSONArray("sources")
        (0 until sources.length()).map { sources.getJSONObject(it) }.first { it.getString("id") == saved.getString("id") }.put("enabled", true)
        store.save(state)
        val originals = MediaRepository(context, store).list(force = true, originalOnly = true, strict = true)
        assertEquals(1, originals.size); assertEquals(original.getString("id"), originals.single().sourceId)
        assertEquals(2, collections.storage().getJSONArray("collections").length())
    }
    @Test fun clearCachePreservesDurableCollectionsAndModelFilesAndDeleteClearsStateCurrentAndCatalog() = withStore { context, store, collections ->
        val original = imported(context, store, false)
        val job = UUID.randomUUID().toString(); val source = collections.append(job, output(context), 0, store.revision()).source
        val model = File(context.filesDir, "ai-runtime/model.bin").apply { parentFile!!.mkdirs(); writeText("model") }
        for (dir in listOf("media", "visual", "upscale", "upscale-results")) File(context.cacheDir, "$dir/cache").apply { parentFile!!.mkdirs(); writeText("cache") }
        assertTrue(collections.storage().getLong("cacheBytes") > 0)
        collections.clearCache()
        assertEquals(0L, collections.storage().getLong("cacheBytes")); assertEquals("model", model.readText())
        assertEquals(1, collections.storage().getJSONArray("collections").getJSONObject(0).getInt("count"))
        val id = source.getString("id")
        store.put("catalog:$id", JSONObject().put("items", JSONArray()))
        store.mutateState { it.put("current", JSONObject().put("sourceId", id)).put("running", true) }
        collections.delete(job)
        val state = store.state(); assertTrue(state.isNull("current")); assertFalse(state.getBoolean("running")); assertNull(store.get("catalog:$id"))
        assertEquals(1, state.getJSONArray("sources").length()); assertEquals(original.getString("id"), state.getJSONArray("sources").getJSONObject(0).getString("id"))
        assertFalse(File(context.filesDir.canonicalFile, "upscaled/$job").exists()); assertEquals("model", model.readText())
    }
    @Test fun legacyDiscoveryAndDeletionNeverTreatNormalImportsAsGenerated() = withStore { context, store, collections ->
        val original = imported(context, store, false); val legacy = imported(context, store, true)
        val entries = collections.storage().getJSONArray("collections")
        assertEquals(1, entries.length()); assertEquals("legacy:${legacy.getString("id")}", entries.getJSONObject(0).getString("id"))
        assertTrue(entries.getJSONObject(0).getString("name").contains("legacy"))
        assertThrows(IllegalStateException::class.java) { collections.delete("legacy:${original.getString("id")}") }
        assertThrows(IllegalArgumentException::class.java) { collections.delete(PhotoImporter.directory(context, original.getString("location")).path) }
        collections.delete("legacy:${legacy.getString("id")}")
        assertEquals(1, store.state().getJSONArray("sources").length()); assertNull(store.session("import", legacy.getString("id"), true))
        assertTrue(PhotoImporter.directory(context, original.getString("location")).isDirectory)
    }
    @Test fun legacyFolderWithoutSourceOrSessionIsRecoverableButOrdinaryImportIsIgnored() = withStore { context, store, collections ->
        val original = imported(context, store, false); val legacy = imported(context, store, true)
        store.removeUpscaleCollection(legacy.getString("location"))
        val entries = collections.storage().getJSONArray("collections")
        assertEquals(1, entries.length()); assertEquals(1, entries.getJSONObject(0).getInt("count"))
        assertTrue(entries.getJSONObject(0).isNull("sourceId"))
        collections.delete(entries.getJSONObject(0).getString("id"))
        assertFalse(PhotoImporter.directory(context, legacy.getString("location")).exists())
        assertTrue(PhotoImporter.directory(context, original.getString("location")).exists())
    }
    @Test fun disconnectingLegacySourceKeepsDurableImagesWhileOrdinaryImportCleanupStillRuns() = withStore { context, store, collections ->
        val original = imported(context, store, false); val legacy = imported(context, store, true)
        val previous = store.state(); val updated = JSONObject(previous.toString()).put("sources", JSONArray())
        store.save(updated); MediaRepository(context, store).cleanup(previous, updated)
        assertTrue(PhotoImporter.directory(context, legacy.getString("location")).isDirectory)
        assertFalse(PhotoImporter.directory(context, original.getString("location")).exists())
        val owned = collections.storage().getJSONArray("collections")
        assertEquals(1, owned.length()); assertEquals(1, owned.getJSONObject(0).getInt("count"))
        assertTrue(owned.getJSONObject(0).isNull("sourceId"))
    }
    @Test fun canonicalPlatformRootAliasesWorkButOwnedSymlinkStillBlocksDeletionBeforeStateChanges() = withStore { context, store, collections ->
        val job = UUID.randomUUID().toString(); val source = collections.append(job, output(context), 0, store.revision()).source
        val outside = Files.createTempDirectory("wallta-outside").toFile().canonicalFile
        val alias = File(outside, "files-alias")
        if (runCatching { Files.createSymbolicLink(alias.toPath(), context.filesDir.toPath()) }.isSuccess) {
            val wrapper = object : ContextWrapper(context) { override fun getFilesDir() = alias }
            assertEquals(1, UpscaleCollections(wrapper, store).storage().getJSONArray("collections").length())
            val escaped = File(context.filesDir.canonicalFile, "upscaled/$job/escape")
            Files.createSymbolicLink(escaped.toPath(), outside.toPath())
            assertThrows(IllegalArgumentException::class.java) { collections.delete(job) }
            assertEquals(source.getString("id"), store.state().getJSONArray("sources").getJSONObject(0).getString("id"))
            escaped.delete(); alias.delete()
        }
        outside.deleteRecursively()
    }
}
