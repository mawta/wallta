package app.wallta

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.ByteArrayOutputStream
import java.io.File

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AndroidUpscaleRenderTest {
    /** Real JPEG with a tiny EXIF orientation IFD, independent of platform metadata writers. */
    private fun jpeg(orientation: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(80, 40, Bitmap.Config.ARGB_8888)
        for (y in 0 until 40) for (x in 0 until 80) bitmap.setPixel(x, y, if (x < 40) Color.RED else Color.BLUE)
        val encoded = ByteArrayOutputStream().use { stream -> check(bitmap.compress(Bitmap.CompressFormat.JPEG, 100, stream)); stream.toByteArray() }
        bitmap.recycle()
        val exif = byteArrayOf(0xff.toByte(), 0xe1.toByte(), 0, 34, 69, 120, 105, 102, 0, 0, 73, 73, 42, 0, 8, 0, 0, 0, 1, 0, 18, 1, 3, 0, 1, 0, 0, 0, orientation.toByte(), 0, 0, 0, 0, 0, 0, 0)
        return encoded.copyOfRange(0, 2) + exif + encoded.copyOfRange(2, encoded.size)
    }
    @Test fun realJpegPortraitRotationAndMirroringAreAppliedBefore4kAndPreviews() {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val store = StateStore(activity); val updater = AndroidUpdater(activity, store)
        val environment = AndroidUpscaleEnvironment(activity, store, updater)
        val folder = java.nio.file.Files.createTempDirectory("wallta-orientation").toFile()
        try {
            for (orientation in listOf(6, 2)) {
                val bytes = jpeg(orientation)
                val input = File(folder, "$orientation.jpg").apply { writeBytes(bytes) }
                val output = File(folder, "$orientation-output.img"); val before = File(folder, "$orientation-before.jpg"); val after = File(folder, "$orientation-after.jpg")
                assertEquals(if (orientation == 6) 1920 to 3840 else 3840 to 1920, environment.render(input, output, before, after))
                val image = BitmapFactory.decodeFile(output.path)
                try {
                    val red = if (orientation == 6) image.getPixel(image.width / 2, image.height / 4) else image.getPixel(image.width * 3 / 4, image.height / 2)
                    assertTrue("EXIF red region must rotate/mirror with the image", Color.red(red) > 200 && Color.blue(red) < 40)
                } finally { image.recycle() }
                val preview = BitmapFactory.decodeFile(before.path)
                try { assertEquals(orientation == 6, preview.height > preview.width) } finally { preview.recycle() }
                assertArrayEquals(bytes, input.readBytes())
            }
        } finally { updater.close(); folder.deleteRecursively() }
    }
}
