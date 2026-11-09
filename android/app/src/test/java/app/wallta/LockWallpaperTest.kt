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
        store.setRunning(true)
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
