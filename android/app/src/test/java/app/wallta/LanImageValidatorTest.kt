package app.wallta

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.ByteArrayOutputStream
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LanImageValidatorTest {
    private fun encoded(format: Bitmap.CompressFormat = Bitmap.CompressFormat.PNG): ByteArray {
        val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
        return try {
            ByteArrayOutputStream().use { out ->
                check(bitmap.compress(format, 100, out))
                out.toByteArray()
            }
        } finally { bitmap.recycle() }
    }

    @Test fun acceptsARealPixelStream() {
        val file = File.createTempFile("wallta-valid-", ".png")
        try {
            for ((format, mime) in listOf(Bitmap.CompressFormat.PNG to "image/png",
                Bitmap.CompressFormat.JPEG to "image/jpeg", Bitmap.CompressFormat.WEBP to "image/webp")) {
                file.writeBytes(encoded(format))
                assertEquals(mime, LanImageValidator.validate(file))
            }
        } finally { file.delete() }
    }

    @Test fun rejectsReadableHeadersWithCorruptPixelData() {
        val file = File.createTempFile("wallta-corrupt-", ".png")
        try {
            // Keep all PNG metadata and framing, but break the IDAT zlib stream.
            val bytes = encoded()
            val idat = (8 until bytes.size - 4).first { index ->
                bytes.copyOfRange(index, index + 4).contentEquals("IDAT".toByteArray())
            }
            bytes[idat + 4] = 0
            val length = java.nio.ByteBuffer.wrap(bytes, idat - 4, 4).int
            val crc = java.util.zip.CRC32().apply { update(bytes, idat, length + 4) }
            java.nio.ByteBuffer.wrap(bytes, idat + length + 4, 4).putInt(crc.value.toInt())
            file.writeBytes(bytes)
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            assertEquals("Fixture must expose readable dimensions", 64, bounds.outWidth)
            assertThrows(IllegalArgumentException::class.java) { LanImageValidator.validate(file) }
        } finally { file.delete() }
    }
}
