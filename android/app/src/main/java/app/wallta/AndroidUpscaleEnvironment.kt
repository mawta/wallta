package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import androidx.exifinterface.media.ExifInterface
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/** Original input and durable imports are resolved natively; the WebView supplies only media IDs. */
internal class AndroidUpscaleEnvironment(private val context: Context, private val store: StateStore, private val updater: AndroidUpdater) : UpscaleManager.Environment {
    override val directory = File(context.cacheDir, "upscale").apply { mkdirs() }
    override fun revision() = store.revision()
    override fun resolve(ids: List<String>): List<Media> {
        val selected = MediaRepository(context, store).list().associateBy { it.id }
        return ids.map { selected[it] ?: error("Selected image is no longer available in an enabled source") }
    }
    override fun input(media: Media) = MediaRepository(context, store).image(media)
    override fun render(input: File, output: File, before: File, after: File): Pair<Int, Int> {
        require(input.length() in 1..(25L * 1024 * 1024)) { "Original image exceeds 25 MB" }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(input.path, bounds)
        val orientation = runCatching { ExifInterface(input.path).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)
        val target = if (orientation in 5..8) UpscalePolicy.target(bounds.outHeight, bounds.outWidth) else UpscalePolicy.target(bounds.outWidth, bounds.outHeight)
        val digest = java.security.MessageDigest.getInstance("SHA-256")
        input.inputStream().use { stream -> val buffer = ByteArray(65536); while (true) { val n = stream.read(buffer); if (n < 0) break; digest.update(buffer, 0, n) } }
        val key = digest.digest().joinToString("") { "%02x".format(it) } + "-fast-v2"
        val cache = File(context.cacheDir, "upscale-results").apply { mkdirs() }
        val cachedOutput = File(cache, "$key.img"); val cachedBefore = File(cache, "$key-before.jpg"); val cachedAfter = File(cache, "$key-after.jpg")
        if (listOf(cachedOutput, cachedBefore, cachedAfter).all { it.isFile && it.length() > 0 }) {
            cachedOutput.copyTo(output); cachedBefore.copyTo(before); cachedAfter.copyTo(after)
            listOf(cachedOutput, cachedBefore, cachedAfter).forEach { it.setLastModified(System.currentTimeMillis()) }
            return target
        }
        require(output.parentFile!!.usableSpace >= 128L * 1024 * 1024) { "At least 128 MB of free storage is required" }
        val raw = BitmapFactory.decodeFile(input.path, BitmapFactory.Options().apply { inSampleSize = UpscalePolicy.sampleSize(bounds.outWidth, bounds.outHeight) }) ?: error("Cannot decode original image")
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
    override fun publish(jobId: String, outputs: List<UpscaleManager.Output>, revision: Long): JSONObject {
        val id = UUID.nameUUIDFromBytes("wallta-4k:$jobId".toByteArray()).toString()
        val location = "wallta-import://${Policy.hash(id)}/$jobId"
        val folder = PhotoImporter.directory(context, location)
        val source = JSONObject().put("id", id).put("name", "Ảnh 4K").put("kind", "local").put("location", location).put("recursive", false).put("enabled", true)
        var committed = false
        try {
            check(folder.mkdirs() || folder.isDirectory) { "Cannot create 4K collection" }
            val items = JSONArray()
            outputs.forEachIndexed { index, output ->
                check(!Thread.currentThread().isInterrupted && store.revision() == revision) { "Sources changed. Start processing again." }
                val name = "${Policy.hash("$jobId:$index")}.img"
                output.file.copyTo(File(folder, name), overwrite = true)
                items.put(JSONObject().put("file", name).put("name", output.name))
            }
            return updater.mutate { synchronized(StateStore.lock) {
                val state = store.state()
                val sources = state.getJSONArray("sources")
                (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == id }?.let { committed = true; return@mutate JSONObject(it.toString()) }
                check(store.revision() == revision && !Thread.currentThread().isInterrupted) { "Sources changed. Start processing again." }
                require(sources.length() < 100) { "Maximum 100 sources" }
                store.beginPending("import", id, JSONObject().put("phase", "ready").put("location", location).put("items", items))
                // save() can commit before a later public read fails: retain the originals in that case.
                sources.put(source); committed = true
                store.save(state)
                source
            } }
        } finally { if (!committed) folder.deleteRecursively() }
    }
}
