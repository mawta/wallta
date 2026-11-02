package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.security.Provider
import java.security.Security
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PhotoPreviewTest {
    private fun fixture(block: (Context, StateStore, Media, File) -> Unit) {
        val provider = object : Provider("WalltaPhotoTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", DriveConnectionCleanupTest.TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        try {
            val context: Context = RuntimeEnvironment.getApplication(); val store = StateStore(context)
            val id = UUID.randomUUID().toString(); val job = UUID.randomUUID().toString()
            val location = "wallta-import://${Policy.hash(id)}/$job"; val name = "${Policy.hash("original")}.img"
            val file = File(PhotoImporter.directory(context, location).apply { mkdirs() }, name)
            image(file, 800, 450)
            store.put("import:$id", JSONObject().put("phase", "ready").put("location", location).put("items", JSONArray().put(JSONObject().put("file", name).put("name", "Original.jpg"))))
            val source = JSONObject().put("id", id).put("name", "Original").put("kind", "local").put("location", location).put("enabled", false)
            store.save(store.state().put("sources", JSONArray().put(source)))
            block(context, store, Media(Policy.hash("$id:$location:$name"), "Original.jpg", "$location/$name", id), file)
        } finally { Security.removeProvider(provider.name) }
    }
    private fun image(file: File, width: Int, height: Int) {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try { file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.JPEG, 95, it)) } } finally { bitmap.recycle() }
    }
    private fun publish(context: Context, store: StateStore, media: Media): Media {
        val file = File(context.cacheDir, "result.jpg"); image(file, 3840, 2160)
        val source = UpscaleCollections(context, store).append(UUID.randomUUID().toString(),
            UpscaleManager.Output(media.id, "Output.jpg", 3840, 2160, file, file, file, media.sourceId), 0, store.revision()).source
        return UpscaleCollections(context, store).media(source).single()
    }
    @Test fun saved4kIsServedAtFullResolutionAndPairingSurvivesNewRepositoryWithDisabledSources() = fixture { context, store, original, _ ->
        val output = publish(context, store, original)
        val preview = MediaRepository(context, StateStore(context)).photoPreview(output.id, output.sourceId)
        assertEquals(3840, preview.getInt("width")); assertEquals(2160, preview.getInt("height"))
        assertEquals(800, preview.getJSONObject("before").getInt("width"))
        assertNotNull(PhotoPreviewCache.served(context, preview.getJSONObject("before").getString("url").substringAfterLast('/')))
        val served = PhotoPreviewCache.served(context, preview.getString("url").substringAfterLast('/'))!!
        assertArrayEquals(UpscaleCollections(context, store).image(output.url).readBytes(), served.readBytes())
        assertEquals(0, MediaRepository(context, store).list().size)
        assertFalse(store.state().getJSONArray("sources").getJSONObject(0).getBoolean("enabled"))
    }
    @Test fun missingOriginalKeepsOutputButDoesNotGuessAnotherOriginal() = fixture { context, store, original, file ->
        val output = publish(context, store, original); file.delete()
        val result = MediaRepository(context, store).photoPreview(output.id, output.sourceId)
        assertTrue(result.getBoolean("comparisonUnavailable")); assertFalse(result.has("before")); assertEquals(3840, result.getInt("width"))
    }
    @Test fun ordinaryPhotoIsBoundedAndRejectsForgedIdsAndPaths() = fixture { context, store, original, file ->
        image(file, 6000, 3000)
        val repo = MediaRepository(context, store); val result = repo.photoPreview(original.id, original.sourceId)
        assertEquals(3000, result.getInt("width")); assertEquals(1500, result.getInt("height"))
        assertFalse(result.has("comparisonUnavailable"))
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(PhotoPreviewCache.served(context, result.getString("url").substringAfterLast('/'))!!.path, bounds)
        assertEquals(result.getInt("width"), bounds.outWidth)
        assertThrows(IllegalArgumentException::class.java) { repo.photoPreview(file.path, original.sourceId) }
        assertThrows(IllegalStateException::class.java) { repo.photoPreview(Policy.hash("forged"), original.sourceId) }
        assertNull(PhotoPreviewCache.served(context, "../result.jpg")); assertNull(PhotoPreviewCache.served(context, "${original.id}.img"))
        assertThrows(IllegalStateException::class.java) { repo.photoPreview(original.id, UUID.randomUUID().toString()) }
        val oversized = File(context.cacheDir.canonicalFile, "photo-preview/${Policy.hash("oversized")}.jpg")
        image(oversized, 3841, 10)
        assertNull(PhotoPreviewCache.served(context, oversized.name))
    }
    @Test fun cacheClearingAndCollectionDeletionCleanPreviewButPreserveOriginal() = fixture { context, store, original, file ->
        val output = publish(context, store, original); val collections = UpscaleCollections(context, store)
        val repo = MediaRepository(context, store); val first = repo.photoPreview(output.id, output.sourceId)
        val served = PhotoPreviewCache.served(context, first.getString("url").substringAfterLast('/'))!!
        collections.clearCache(); assertFalse(served.exists()); assertTrue(file.exists())
        repo.photoPreview(output.id, output.sourceId)
        collections.delete(UpscaleCollections.locationId(output.url.substringBeforeLast('/'))!!)
        assertFalse(served.exists()); assertTrue(file.exists())
    }
    @Test fun oldUnpairedOutputIsPreviewOnly() = fixture { context, store, original, _ ->
        val output = publish(context, store, original)
        val job = UpscaleCollections.locationId(output.url.substringBeforeLast('/'))!!
        val manifestFile = File(context.filesDir.canonicalFile, "upscaled/$job/collection.json")
        val manifest = JSONObject(manifestFile.readText()); manifest.getJSONArray("items").getJSONObject(0).remove("original"); manifestFile.writeText(manifest.toString())
        val result = MediaRepository(context, store).photoPreview(output.id, output.sourceId)
        assertTrue(result.getBoolean("comparisonUnavailable")); assertFalse(result.has("before"))
    }
    @Test fun removedOriginalSourceKeepsOutputPreviewAvailable() = fixture { context, store, original, _ ->
        val output = publish(context, store, original)
        val state = store.state(); val sources = state.getJSONArray("sources")
        state.put("sources", JSONArray().apply { (0 until sources.length()).map { sources.getJSONObject(it) }.filter { it.optString("id") != original.sourceId }.forEach { put(it) } })
        store.save(state)
        val result = MediaRepository(context, store).photoPreview(output.id, output.sourceId)
        assertEquals(3840, result.getInt("width")); assertTrue(result.getBoolean("comparisonUnavailable"))
    }
    @Test fun disconnectedOriginalProviderKeepsSavedOutputPreviewAvailable() = fixture { context, store, original, _ ->
        val output = publish(context, store, original)
        val state = store.state(); val sources = state.getJSONArray("sources")
        (0 until sources.length()).map { sources.getJSONObject(it) }.first { it.optString("id") == original.sourceId }
            .put("kind", "drive").put("location", "disconnected-folder")
        store.save(state)
        val result = MediaRepository(context, store).photoPreview(output.id, output.sourceId)
        assertEquals(3840, result.getInt("width")); assertTrue(result.getBoolean("comparisonUnavailable")); assertFalse(result.has("before"))
    }
    @Test fun smallCacheLimitKeepsOutputUrlUsableAndDegradesComparisonWhenPairCannotFit() = fixture { context, store, original, _ ->
        val output = publish(context, store, original)
        UpscaleCollections(context, store).image(output.url).appendBytes(ByteArray(9 * 1024 * 1024))
        val state = store.state(); state.getJSONObject("settings").put("cacheLimitMb", 16); store.save(state)
        val result = MediaRepository(context, store).photoPreview(output.id, output.sourceId)
        assertNotNull(PhotoPreviewCache.served(context, result.getString("url").substringAfterLast('/')))
        assertTrue(result.optBoolean("comparisonUnavailable")); assertFalse(result.has("before"))
    }
    @Test fun repeatedPreviewWithCachedInputStillTrimsCacheIncludingViewerBytes() = fixture { context, store, original, _ ->
        val state = store.state(); state.getJSONObject("settings").put("cacheLimitMb", 16); store.save(state)
        val repo = MediaRepository(context, store); repo.photoPreview(original.id, original.sourceId)
        val stale = repo.cacheFile(Policy.hash("stale-cache")).apply { writeBytes(ByteArray(17 * 1024 * 1024)) }
        repo.photoPreview(original.id, original.sourceId)
        assertFalse(stale.exists())
    }
    @Test fun legacyImportWithoutPairingIsPreviewOnly() = fixture { context, store, _, file ->
        val job = UUID.randomUUID().toString(); val sourceId = UUID.nameUUIDFromBytes("wallta-4k:$job".toByteArray()).toString()
        val location = "wallta-import://${Policy.hash(sourceId)}/$job"; val name = "${Policy.hash("legacy")}.img"
        file.copyTo(File(PhotoImporter.directory(context, location).apply { mkdirs() }, name))
        store.put("import:$sourceId", JSONObject().put("phase", "ready").put("location", location).put("items", JSONArray().put(JSONObject().put("file", name).put("name", "Old-4K.jpg"))))
        val state = store.state(); state.getJSONArray("sources").put(JSONObject().put("id", sourceId).put("name", "Legacy").put("kind", "local").put("location", location).put("enabled", false)); store.save(state)
        val result = MediaRepository(context, store).photoPreview(Policy.hash("$sourceId:$location:$name"), sourceId)
        assertEquals(800, result.getInt("width")); assertTrue(result.getBoolean("comparisonUnavailable")); assertFalse(result.has("before"))
    }
}
