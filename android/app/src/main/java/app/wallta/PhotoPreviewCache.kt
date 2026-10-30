package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import androidx.exifinterface.media.ExifInterface
import org.json.JSONObject
import java.io.File

/** Full 4K viewer cache; never expose unbounded originals to WebView's decoder. */
internal object PhotoPreviewCache {
    private val decodeLock = Any()
    private fun file(context: Context, id: String): File {
        require(id.matches(Regex("[a-f0-9]{64}")))
        return File(UpscaleFiles.root(File(context.cacheDir.canonicalFile, "photo-preview"), true), "$id.jpg")
    }
    fun remove(context: Context, id: String) { file(context, id).delete() }
    fun length(context: Context, id: String) = file(context, id).length()
    fun served(context: Context, path: String): File? {
        if (!path.matches(Regex("[a-f0-9]{64}\\.jpg"))) return null
        return runCatching {
            file(context, path.substringBefore('.')).takeIf { target ->
                UpscaleFiles.plain(target, false) && target.length() in 1..64L * 1024 * 1024 &&
                    BitmapFactory.Options().apply { inJustDecodeBounds = true; BitmapFactory.decodeFile(target.path, this) }
                        .let { it.outMimeType == "image/jpeg" && it.outWidth in 1..3840 && it.outHeight in 1..3840 }
            }
        }.getOrNull()
    }
    fun prepare(context: Context, store: StateStore, media: Media, original: File, snapshot: Pair<Long, Long>): JSONObject = synchronized(decodeLock) {
        val target = file(context, media.id)
        fun ensureCurrent() { check(!Thread.currentThread().isInterrupted && store.revision() == snapshot.first && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == snapshot.second && original.isFile) { "Preview operation cancelled" } }
        synchronized(StateStore.lock) { ensureCurrent() }
        if (served(context, target.name) == null) {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(original.path, bounds)
            require(original.length() in 1..64L * 1024 * 1024 && bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) { "Invalid or oversized image" }
            val orientation = runCatching { ExifInterface(original.path).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }.getOrDefault(1)
            val temporary = File.createTempFile("photo-", ".tmp", target.parentFile)
            try {
                if (bounds.outMimeType == "image/jpeg" && bounds.outWidth <= 3840 && bounds.outHeight <= 3840 && orientation in listOf(0, 1)) {
                    original.copyTo(temporary, overwrite = true)
                } else {
                    var sample = 1
                    while ((bounds.outWidth.toLong() + sample - 1) / sample * ((bounds.outHeight.toLong() + sample - 1) / sample) > 16_777_216 || maxOf(bounds.outWidth, bounds.outHeight) / sample > 8192) sample *= 2
                    val raw = BitmapFactory.decodeFile(original.path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: error("Cannot decode image")
                    var rotated: Bitmap? = null; var scaled: Bitmap? = null
                    try {
                        val matrix = Matrix().apply {
                            when (orientation) {
                                2 -> setScale(-1f, 1f); 3 -> setRotate(180f); 4 -> setScale(1f, -1f)
                                5 -> { setRotate(90f); postScale(-1f, 1f) }; 6 -> setRotate(90f)
                                7 -> { setRotate(270f); postScale(-1f, 1f) }; 8 -> setRotate(270f)
                            }
                        }
                        rotated = if (matrix.isIdentity) raw else Bitmap.createBitmap(raw, 0, 0, raw.width, raw.height, matrix, true)
                        val longest = maxOf(rotated.width, rotated.height)
                        scaled = if (longest <= 3840) rotated else Bitmap.createScaledBitmap(rotated, maxOf(1, rotated.width * 3840 / longest), maxOf(1, rotated.height * 3840 / longest), true)
                        temporary.outputStream().use { check(scaled.compress(Bitmap.CompressFormat.JPEG, 95, it)) { "Cannot encode photo preview" } }
                    } finally { scaled?.takeIf { it !== rotated && it !== raw }?.recycle(); rotated?.takeIf { it !== raw }?.recycle(); raw.recycle() }
                }
                synchronized(StateStore.lock) { ensureCurrent(); UpscaleFiles.atomicWrite(target, temporary.readBytes()) }
            } finally { temporary.delete() }
        }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(target.path, bounds)
        synchronized(StateStore.lock) { ensureCurrent() }
        JSONObject().put("url", "${TrustedWebView.origin}/photo-preview/${target.name}").put("width", bounds.outWidth).put("height", bounds.outHeight)
    }
}
