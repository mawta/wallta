package app.wallta

import android.app.WallpaperManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import java.io.File
import java.security.Provider
import java.security.Security
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [28], manifest = Config.NONE, shadows = [LockWallpaperTest.WallpaperSink::class])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LockWallpaperTest {
    @Test fun changingSelectedLockOrItsProviderClearsOnlyItsNativeStatus() = withStore { context, store ->
        val first = source(context, store, "First")
        val second = source(context, store, "Second")
        val state = store.state().put("sources", JSONArray().put(first).put(second))
        state.getJSONObject("settings").put("lockSourceId", first.getString("id"))
        store.save(state); WallpaperEngine.applyLock(context, manual = true)
        assertFalse(store.state().isNull("lockCurrent"))
        val selected = store.state(); selected.getJSONObject("settings").put("lockSourceId", second.getString("id"))
        store.save(selected)
        assertTrue(store.state().isNull("lockCurrent"))
        WallpaperEngine.applyLock(context, manual = true)
        val edited = store.state()
        edited.getJSONArray("sources").getJSONObject(1).put("recursive", true)
        store.save(edited)
        assertTrue(store.state().isNull("lockCurrent"))
        assertTrue(store.state().isNull("lockLastChanged"))
    }
    @Test fun visibleDecoderPinsSurviveTrimAndClearUntilReleased() = withStore { context, store ->
        val repo = MediaRepository(context, store)
        val active = Policy.hash("active"); val next = Policy.hash("next")
        repo.cacheFile(active).writeBytes(ByteArray(512)); repo.cacheFile(next).writeBytes(ByteArray(512))
        ActiveMediaPins.acquire(active)
        try {
            repo.trim(512, keep = next)
            assertTrue(repo.cacheFile(active).isFile)
            assertTrue(repo.cacheFile(next).isFile)
            repo.clear()
            assertTrue(repo.cacheFile(active).isFile)
            assertFalse(repo.cacheFile(next).isFile)
        } finally { ActiveMediaPins.release(active) }
        repo.clear()
        assertFalse(repo.cacheFile(active).isFile)
    }
    @Test fun configuredHomeAndLockSourcesStaySeparateWhenBothAreEnabled() = withStore { context, store ->
        val home = source(context, store, "Home")
        val lock = source(context, store, "Lock")
        val state = store.state().put("sources", JSONArray().put(home).put(lock))
        state.getJSONObject("settings").put("homeSourceId", home.getString("id")).put("lockSourceId", lock.getString("id")).put("order", "sequential")
        store.save(state)
        repeat(3) { WallpaperEngine.apply(context, manual = true); WallpaperEngine.applyLock(context, manual = true) }
        assertEquals(home.getString("id"), store.state().getJSONObject("current").getString("sourceId"))
        assertEquals(lock.getString("id"), store.state().getJSONObject("lockCurrent").getString("sourceId"))
        val public = store.state(true)
        assertEquals("", public.getJSONObject("lockCurrent").getString("url"))
        assertFalse(store.state().getJSONObject("lockCurrent").getString("url").isEmpty())
    }
    @Test fun legacyLockMigratesButNewLockScheduleIsIndependentOfHome() = withStore { context, store ->
        val lock = source(context, store, "Legacy")
        val state = store.state().put("sources", JSONArray().put(lock)).put("running", true)
        val settings = state.getJSONObject("settings").put("lockSourceId", lock.getString("id"))
        settings.remove("lockRunning"); settings.remove("lockIntervalSeconds")
        store.save(state)
        assertTrue(store.state().getJSONObject("settings").getBoolean("lockRunning"))
        assertEquals(900, store.state().getJSONObject("settings").getInt("lockIntervalSeconds"))
        store.setRunning(false)
        WallpaperEngine.applyLock(context)
        assertFalse(store.state().getBoolean("running"))
        assertEquals(lock.getString("id"), store.state().getJSONObject("lockCurrent").getString("sourceId"))
    }
    @Test fun staleSavesCannotForgeLockSuccessAndRemovingSourceClearsLockState() = withStore { context, store ->
        val lock = source(context, store, "Lock")
        val state = store.state().put("sources", JSONArray().put(lock))
        state.getJSONObject("settings").put("lockSourceId", lock.getString("id")).put("lockRunning", true).put("lockIntervalSeconds", 5)
        store.save(state)
        val stale = store.state(true)
        WallpaperEngine.applyLock(context, manual = true)
        val current = store.state().getJSONObject("lockCurrent").getString("id")
        store.save(stale)
        assertEquals(current, store.state().getJSONObject("lockCurrent").getString("id"))
        assertEquals(900, store.state().getJSONObject("settings").getInt("lockIntervalSeconds"))
        store.save(store.state().put("sources", JSONArray()))
        assertFalse(store.state().getJSONObject("settings").getBoolean("lockRunning"))
        assertTrue(store.state().isNull("lockCurrent"))
    }
    @Test fun appliesOnlyTheSavedSourceToTheLockScreen() = withStore { context, store ->
        val home = source(context, store, "Home")
        val lock = source(context, store, "Lock")
        val state = store.state().put("sources", JSONArray().put(home).put(lock))
        state.getJSONObject("settings").put("lockSourceId", lock.getString("id")).put("order", "sequential")
        store.save(state)
        val manager = WallpaperManager.getInstance(context)

        WallpaperEngine.apply(context, manual = true, sourceId = home.getString("id"))
        val homeId = manager.getWallpaperId(WallpaperManager.FLAG_SYSTEM)
        val lockId = manager.getWallpaperId(WallpaperManager.FLAG_LOCK)
        WallpaperEngine.applyLock(context, manual = true)

        assertEquals(homeId, manager.getWallpaperId(WallpaperManager.FLAG_SYSTEM))
        assertNotEquals(lockId, manager.getWallpaperId(WallpaperManager.FLAG_LOCK))
        assertEquals(home.getString("id"), store.state().getJSONObject("current").getString("sourceId"))
        val lockMedia = MediaRepository(context, store).list(sourceIds = setOf(lock.getString("id"))).single()
        assertEquals(lockMedia.id, store.get("lockRotation")!!.getString("current"))

        val manualLockId = manager.getWallpaperId(WallpaperManager.FLAG_LOCK)
        WallpaperEngine.applyLock(context)
        assertEquals(manualLockId, manager.getWallpaperId(WallpaperManager.FLAG_LOCK))
        val enabled = store.state()
        enabled.getJSONObject("settings").put("lockRunning", true)
        store.save(enabled)
        WallpaperEngine.applyLock(context)
        assertNotEquals(manualLockId, manager.getWallpaperId(WallpaperManager.FLAG_LOCK))
        assertEquals(homeId, manager.getWallpaperId(WallpaperManager.FLAG_SYSTEM))

        val next = store.state().put("sources", JSONArray().put(home))
        store.save(next)
        assertEquals("", store.state().getJSONObject("settings").optString("lockSourceId"))
        val appliedLockId = manager.getWallpaperId(WallpaperManager.FLAG_LOCK)
        assertThrows(IllegalArgumentException::class.java) { WallpaperEngine.applyLock(context, manual = true) }
        WallpaperEngine.applyLock(context)
        assertEquals(appliedLockId, manager.getWallpaperId(WallpaperManager.FLAG_LOCK))
    }

    // Robolectric's bundled wallpaper shadow references classes absent on Android 9.
    @Implements(WallpaperManager::class)
    class WallpaperSink {
        private val ids = mutableMapOf<Int, Int>()
        private var nextId = 0

        @Implementation protected fun isWallpaperSupported() = true
        @Implementation protected fun isSetWallpaperAllowed() = true
        @Implementation protected fun getDesiredMinimumWidth() = 720
        @Implementation protected fun getDesiredMinimumHeight() = 720
        @Implementation protected fun getWallpaperId(which: Int) = ids[which] ?: 0
        @Suppress("UNUSED_PARAMETER")
        @Implementation protected fun setBitmap(image: Bitmap, crop: Rect?, backup: Boolean, which: Int): Int {
            val id = ++nextId
            for (target in listOf(WallpaperManager.FLAG_SYSTEM, WallpaperManager.FLAG_LOCK)) {
                if (which and target != 0) ids[target] = id
            }
            return id
        }
    }

    private fun source(context: Context, store: StateStore, name: String): JSONObject {
        val id = UUID.randomUUID().toString()
        val location = "wallta-import://${Policy.hash(id)}/${UUID.randomUUID()}"
        val fileName = "${Policy.hash(name)}.img"
        val directory = PhotoImporter.directory(context, location).apply { mkdirs() }
        val bitmap = Bitmap.createBitmap(40, 30, Bitmap.Config.ARGB_8888)
        try {
            File(directory, fileName).outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.JPEG, 90, it)) }
        } finally { bitmap.recycle() }
        val entry = JSONObject().put("file", fileName).put("name", "$name.jpg")
        store.put("import:$id", JSONObject().put("phase", "ready").put("location", location).put("items", JSONArray().put(entry)))
        return JSONObject().put("id", id).put("kind", "local").put("name", name).put("location", location).put("enabled", true)
    }

    private fun withStore(block: (Context, StateStore) -> Unit) {
        val provider = object : Provider("WalltaLockTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", DriveConnectionCleanupTest.TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        try {
            val context = RuntimeEnvironment.getApplication()
            block(context, StateStore(context))
        } finally { Security.removeProvider(provider.name) }
    }
}
