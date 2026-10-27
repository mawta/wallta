package app.wallta

import android.graphics.BitmapFactory
import java.io.File

internal object LanImageValidator {
    fun validate(file: File): String {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.absolutePath, bounds)
        require(bounds.outMimeType in listOf("image/jpeg", "image/png", "image/webp") && bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) {
            "Only valid JPEG, PNG or WebP images up to 120 MP are supported"
        }
        return bounds.outMimeType
    }
}
