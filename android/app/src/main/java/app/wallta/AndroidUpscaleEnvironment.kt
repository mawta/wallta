package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import androidx.exifinterface.media.ExifInterface
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Original input and durable imports are resolved natively; the WebView supplies only media IDs. */
internal class AndroidUpscaleEnvironment(private val context: Context, private val store: StateStore, private val updater: AndroidUpdater) : UpscaleManager.Environment {
    override val directory = File(context.cacheDir.canonicalFile, "upscale").apply { mkdirs() }
    private val requestTag = "wallta-upscale-${java.util.UUID.randomUUID()}"
    private val collections = UpscaleCollections(context, store)
    override fun revision() = updater.mutate { store.revision() }
    override fun snapshot(sourceId: String?): List<Media> {
        updater.mutate {
            if (sourceId != null) {
                val sources = store.state().getJSONArray("sources")
                require((0 until sources.length()).map { sources.getJSONObject(it) }.any { it.optString("id") == sourceId && it.optBoolean("enabled", true) && !collections.isManaged(it) }) { "Choose an enabled original source" }
            }
        }
        return Net.scoped(requestTag) { MediaRepository(context, store).list(force = true, sourceIds = sourceId?.let { setOf(it) }, originalOnly = true, strict = true) }
    }
    override fun storage(): JSONObject = updater.mutate { collections.storage() }
    override fun clearCache(): JSONObject = updater.mutate { collections.clearCache() }
    override fun deleteCollection(collectionId: String): JSONObject = updater.mutate { collections.delete(collectionId) }
    override fun resolve(ids: List<String>): List<Media> {
        val selected = Net.scoped(requestTag) { MediaRepository(context, store).list() }.associateBy { it.id }
        return ids.map { selected[it] ?: error("Selected image is no longer available in an enabled source") }
    }
    override fun cancelWork() { Net.cancel(requestTag) }
    override fun input(media: Media): File = try { Net.scoped(requestTag) { MediaRepository(context, store).image(media) } }
    catch (e: IllegalArgumentException) {
        if (e.message?.contains("Invalid or oversized image") == true || e.message == "Response exceeds size limit") throw UpscaleManager.ImageFailure(e.message ?: "Invalid image", e)
        throw e
    }
    override fun render(input: File, output: File, before: File, after: File): Pair<Int, Int> {
        if (input.length() !in 1..(25L * 1024 * 1024)) throw UpscaleManager.ImageFailure("Original image exceeds 25 MB or is empty")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(input.path, bounds)
        val orientation = runCatching { ExifInterface(input.path).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)
        val target = try { if (orientation in 5..8) UpscalePolicy.target(bounds.outHeight, bounds.outWidth) else UpscalePolicy.target(bounds.outWidth, bounds.outHeight) } catch (e: IllegalArgumentException) { throw UpscaleManager.ImageFailure("Invalid or oversized image", e) }
        val digest = java.security.MessageDigest.getInstance("SHA-256")
        input.inputStream().use { stream -> val buffer = ByteArray(65536); while (true) { val n = stream.read(buffer); if (n < 0) break; digest.update(buffer, 0, n) } }
        val key = digest.digest().joinToString("") { "%02x".format(it) } + "-fast-v2"
        val cache = UpscaleFiles.root(File(context.cacheDir.canonicalFile, "upscale-results"), true)
        val cachedOutput = File(cache, "$key.img"); val cachedBefore = File(cache, "$key-before.jpg"); val cachedAfter = File(cache, "$key-after.jpg")
        if (listOf(cachedOutput, cachedBefore, cachedAfter).all { UpscaleFiles.plain(it, false) && it.length() > 0 }) {
            cachedOutput.copyTo(output); cachedBefore.copyTo(before); cachedAfter.copyTo(after)
            listOf(cachedOutput, cachedBefore, cachedAfter).forEach { it.setLastModified(System.currentTimeMillis()) }
            return target
        }
        check(output.parentFile!!.usableSpace >= 128L * 1024 * 1024) { "At least 128 MB of free storage is required" }
        val raw = BitmapFactory.decodeFile(input.path, BitmapFactory.Options().apply { inSampleSize = UpscalePolicy.sampleSize(bounds.outWidth, bounds.outHeight) }) ?: throw UpscaleManager.ImageFailure("Cannot decode original image")
        val decoded = try {
            val matrix = Matrix().apply {
                when (orientation) {
                    2 -> setScale(-1f, 1f)
                    3 -> setRotate(180f)
                    4 -> setScale(1f, -1f)
                    5 -> { setRotate(90f); postScale(-1f, 1f) }
                    6 -> setRotate(90f)
                    7 -> { setRotate(270f); postScale(-1f, 1f) }
                    8 -> setRotate(270f)
                }
            }
            if (matrix.isIdentity) raw else Bitmap.createBitmap(raw, 0, 0, raw.width, raw.height, matrix, true)
        } catch (e: Throwable) { raw.recycle(); throw e }
        if (decoded !== raw) raw.recycle()
        var scaled: Bitmap? = null
        fun jpeg(bitmap: Bitmap, file: File, quality: Int) { file.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.JPEG, quality, it)) { "Cannot encode 4K image" } } }
        fun preview(bitmap: Bitmap, file: File) {
            val size = MediaSafety.visualSize(bitmap.width, bitmap.height)
            val small = if (size == (bitmap.width to bitmap.height)) bitmap else Bitmap.createScaledBitmap(bitmap, size.first, size.second, true)
            try { jpeg(small, file, 88) } finally { if (small !== bitmap) small.recycle() }
        }
        try {
            check(!Thread.currentThread().isInterrupted) { "4K processing cancelled" }
            preview(decoded, before)
            scaled = Bitmap.createScaledBitmap(decoded, target.first, target.second, true)
            jpeg(scaled, output, 95)
            preview(scaled, after)
            check(!Thread.currentThread().isInterrupted) { "4K processing cancelled" }
            output.copyTo(cachedOutput, overwrite = true); before.copyTo(cachedBefore, overwrite = true); after.copyTo(cachedAfter, overwrite = true)
            var bytes = cache.listFiles()?.sumOf { it.length() } ?: 0L
            cache.listFiles()?.sortedBy { it.lastModified() }?.filter { !it.name.startsWith(key) }?.forEach { if (bytes > 256L * 1024 * 1024) { bytes -= it.length(); it.delete() } }
            return target
        } finally { scaled?.takeIf { it !== decoded }?.recycle(); decoded.recycle() }
    }
    override fun append(jobId: String, output: UpscaleManager.Output, index: Int, revision: Long): UpscaleManager.Publication =
        updater.mutate { collections.append(jobId, output, index, revision) }
}
