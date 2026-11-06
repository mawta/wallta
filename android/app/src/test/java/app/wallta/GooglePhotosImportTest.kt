package app.wallta

import android.graphics.Bitmap
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.ByteArrayOutputStream
import java.security.Provider
import java.security.Security

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GooglePhotosImportTest {
    @Test fun corruptSelectedPixelsCannotReplaceSavedImport() {
        val provider = object : Provider("WalltaPickerTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", DriveConnectionCleanupTest.TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        try {
            val context = RuntimeEnvironment.getApplication(); val store = StateStore(context)
            val oldLocation = "wallta-import://${Policy.hash("p")}/11111111-1111-1111-1111-111111111111"
            val oldFolder = PhotoImporter.directory(context, oldLocation).apply { mkdirs() }
            val original = java.io.File(oldFolder, "original.img").apply { writeText("saved-photo") }
            store.put("import:p", JSONObject().put("phase", "ready").put("location", oldLocation))
            val connection = store.beginPending("googleConnection", "p", JSONObject().put("kind", "photos").put("phase", "selecting"))
            val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
            val bytes = try { ByteArrayOutputStream().use { out -> check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)); out.toByteArray() } } finally { bitmap.recycle() }
            val idat = (8 until bytes.size - 4).first { bytes.copyOfRange(it, it + 4).contentEquals("IDAT".toByteArray()) }
            bytes[idat + 4] = 0
            val length = java.nio.ByteBuffer.wrap(bytes, idat - 4, 4).int
            val crc = java.util.zip.CRC32().apply { update(bytes, idat, length + 4) }
            java.nio.ByteBuffer.wrap(bytes, idat + length + 4, 4).putInt(crc.value.toInt())
            val picked = JSONObject().put("id", "photo").put("mediaFile", JSONObject().put("mimeType", "image/png").put("filename", "photo.png").put("baseUrl", "https://lh3.googleusercontent.com/photo"))
            assertThrows(IllegalArgumentException::class.java) { PhotoImporter(context, store).importGoogle("p", listOf(picked), "a@example.com", connection.getString("nonce")) { bytes } }
            assertEquals("saved-photo", original.readText())
            assertEquals(oldLocation, store.session("import", "p", true)?.getString("location")); assertNull(store.get("importPending:p"))
        } finally { Security.removeProvider(provider.name) }
    }
}
