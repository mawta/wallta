package app.wallta

import android.graphics.BitmapFactory
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.os.Build
import java.io.File

internal object LanImageValidator {
    fun validate(file: File): String {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.absolutePath, bounds)
        require(bounds.outMimeType in listOf("image/jpeg", "image/png", "image/webp") && bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) {
            "Only valid JPEG, PNG or WebP images up to 120 MP are supported"
        }
        try { ImageContainer.validate(file, bounds.outMimeType) }
        catch (e: java.io.IOException) { throw IllegalArgumentException("Incomplete or corrupt image", e) }
        var sample = 1
        while (((bounds.outWidth.toLong() + sample - 1) / sample) *
            ((bounds.outHeight.toLong() + sample - 1) / sample) > 2_000_000) sample *= 2
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inScaled = false
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        // Header parsing alone does not verify the compressed pixel stream.
        val decoded = try {
            if (Build.VERSION.SDK_INT >= 28) {
                ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, _, _ ->
                    decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                    decoder.setTargetSampleSize(sample)
                    decoder.setOnPartialImageListener { false }
                }
            } else BitmapFactory.decodeFile(file.absolutePath, options)
        } catch (e: java.io.IOException) {
            throw IllegalArgumentException("Image pixel data cannot be decoded", e)
        } catch (e: OutOfMemoryError) {
            throw IllegalArgumentException("Not enough memory to validate this image", e)
        }
        requireNotNull(decoded) { "Image pixel data cannot be decoded" }
        decoded.recycle()
        return bounds.outMimeType
    }
}
