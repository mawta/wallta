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
@Config(sdk = [28], manifest = Config.NONE)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LanImageValidatorTest {
    private fun png(): ByteArray {
        val bitmap = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888)
        return try {
            ByteArrayOutputStream().use { out ->
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, out))
                out.toByteArray()
            }
        } finally { bitmap.recycle() }
    }

    @Test fun acceptsARealPixelStream() {
        val file = File.createTempFile("wallta-valid-", ".png")
        try {
            file.writeBytes(png())
            assertEquals("image/png", LanImageValidator.validate(file))
        } finally { file.delete() }
    }

    @Test fun rejectsReadableHeadersWithoutPixelData() {
        val file = File.createTempFile("wallta-corrupt-", ".png")
        try {
            // Keep IHDR and the IDAT header, but remove the compressed pixel stream.
            file.writeBytes(png().copyOf(41))
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            assertEquals("Fixture must expose readable dimensions", 64, bounds.outWidth)
            assertThrows(IllegalArgumentException::class.java) { LanImageValidator.validate(file) }
        } finally { file.delete() }
    }
}
