package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.File

/** Only this bounded JPEG derivative is ever exposed to a WebView decoder. */
object VisualCache {
    private val decodeLock = Any()
    fun file(context: Context, id: String): File {
        require(id.matches(Regex("[a-f0-9]{64}")))
        return File(File(context.cacheDir, "visual").apply { mkdirs() }, "$id.jpg")
    }
    fun prepare(context: Context, store: StateStore, media: Media, original: File, snapshot: Pair<Long, Long>): File = synchronized(decodeLock) {
        val target = file(context, media.id)
        fun ensureCurrent() { check(!Thread.currentThread().isInterrupted && store.revision() == snapshot.first && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == snapshot.second && original.isFile) { "Preview operation cancelled" } }
        synchronized(StateStore.lock) { ensureCurrent(); if (target.isFile) return target }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(original.path, bounds)
        val sample = MediaSafety.sampleSize(bounds.outWidth, bounds.outHeight)
        val decoded = BitmapFactory.decodeFile(original.path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: error("Cannot decode image")
        var visual: Bitmap? = null
        val temporary = File.createTempFile("visual-", ".tmp", target.parentFile)
        try {
            val size = MediaSafety.visualSize(decoded.width, decoded.height)
            val scaled = if (size.first == decoded.width && size.second == decoded.height) decoded else Bitmap.createScaledBitmap(decoded, size.first, size.second, true)
            visual = scaled
            temporary.outputStream().use { check(scaled.compress(Bitmap.CompressFormat.JPEG, 88, it)) { "Cannot encode preview" } }
            synchronized(StateStore.lock) {
                ensureCurrent()
                require(temporary.renameTo(target)) { "Cannot cache preview" }
            }
            MediaRepository(context, store).trim(store.state().getJSONObject("settings").optInt("cacheLimitMb", 256).toLong() * 1024 * 1024, media.id)
            target
        } finally { temporary.delete(); visual?.takeIf { it !== decoded }?.recycle(); decoded.recycle() }
    }
}
