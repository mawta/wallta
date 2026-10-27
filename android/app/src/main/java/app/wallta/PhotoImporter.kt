package app.wallta

import android.content.Context
import android.graphics.BitmapFactory
import android.provider.OpenableColumns
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.URI

class PhotoImporter(private val context: Context, private val store: StateStore = StateStore(context)) {
    companion object {
        fun directory(context: Context, location: String): File {
            val uri = URI(location)
            require(uri.scheme == "wallta-import" && uri.host?.matches(Regex("[a-f0-9]{64}")) == true && uri.path.matches(Regex("/[a-f0-9-]{36}")) && uri.query == null && uri.fragment == null)
            val base = File(context.filesDir, "imports").canonicalFile
            val file = File(base, uri.host + uri.path).canonicalFile
            require(file.path.startsWith(base.path + File.separator)); return file
        }
    }
    fun begin(id: String): JSONObject {
        require(id.length in 1..128)
        val activity = context as? MainActivity ?: error("Open Wallta to choose photos")
        cancel(id)
        val pending = store.beginPending("import", id, JSONObject().put("phase", "selecting"))
        val location = "wallta-import://${Policy.hash(id)}/${pending.getString("nonce")}"
        pending.put("location", location); store.saveSession("import", id, pending)
        val folder = directory(context, location); folder.mkdirs()
        try {
            val uris = activity.pickImages(id)
            var total = 0L; val items = JSONArray()
            for (uri in uris.take(50)) {
                check(store.session("import", id)?.optString("nonce") == pending.getString("nonce")) { "Photo import cancelled" }
                require(uri.scheme == "content") { "Image picker must return a content URI" }
                val bytes = context.contentResolver.openInputStream(uri)?.use { Net.bounded(it, 25 * 1024 * 1024) } ?: error("Cannot read selected image")
                total += bytes.size; require(total <= 256L * 1024 * 1024) { "Import exceeds 256 MB. Choose fewer photos." }
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                require(bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) { "Selected file is not a supported image" }
                val filename = "${Policy.hash(uri.toString())}.img"
                val name = runCatching { context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { if (it.moveToFirst()) it.getString(0) else "Photo" } }.getOrNull() ?: "Photo"
                synchronized(StateStore.lock) {
                    check(store.session("import", id)?.optString("nonce") == pending.getString("nonce")) { "Photo import cancelled" }
                    File(folder, filename).writeBytes(bytes)
                }
                items.put(JSONObject().put("file", filename).put("name", name))
            }
            require(items.length() > 0) { "No supported images selected" }
            pending.put("phase", "ready").put("items", items); store.saveSession("import", id, pending)
            return JSONObject().put("phase", "ready").put("location", location).put("sourceKind", "local").put("message", "Imported ${items.length()} photos to this device. Save this source to keep them.")
        } catch (e: Exception) { folder.deleteRecursively(); store.removeSession("import", id, pending); throw e }
    }
    fun cancel(id: String) {
        (context as? MainActivity)?.cancelImages(id)
        val pending = store.takePending("import", id) ?: return
        pending.optString("location").takeIf { it.isNotEmpty() }?.let { directory(context, it).deleteRecursively() }
    }
    fun remove(id: String, location: String) {
        cancel(id); directory(context, location).deleteRecursively()
        if (store.session("import", id, true)?.optString("location") == location) store.put("import:$id", null)
    }
}
