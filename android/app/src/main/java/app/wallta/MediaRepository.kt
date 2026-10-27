package app.wallta

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.util.Base64
import androidx.documentfile.provider.DocumentFile
import org.json.JSONArray
import org.json.JSONObject
import org.xmlpull.v1.XmlPullParser
import android.util.Xml
import java.io.File
import java.io.StringReader
import java.net.URI

class MediaRepository(private val context: Context, private val store: StateStore = StateStore(context)) {
    private val dir = File(context.cacheDir, "media").apply { mkdirs() }
    fun networkAllowed(settings: JSONObject): Boolean {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
        return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && (!settings.optBoolean("wifiOnly") || caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
    }
    fun list(force: Boolean = false): List<Media> {
        val state = store.state(); val settings = state.getJSONObject("settings"); val sources = state.getJSONArray("sources")
        val result = mutableListOf<Media>(); val errors = mutableListOf<String>()
        for (i in 0 until sources.length()) {
            val s = sources.getJSONObject(i); if (!s.optBoolean("enabled", true)) continue
            val id = s.getString("id"); val key = "catalog:$id"; val cached = store.get(key)
            val existing = cached?.optJSONArray("items")?.let { a -> (0 until a.length()).map { Media.from(a.getJSONObject(it)) } } ?: emptyList()
            val online = networkAllowed(settings)
            if (s.getString("kind") != "local" && !online) {
                // Photos never enter the offline fallback catalog.
                if (s.getString("kind") != "photos") result += existing.filter { cacheFile(it.id).exists() }
                continue
            }
            if (!force && s.getString("kind") != "local" && cached != null && System.currentTimeMillis() - cached.optLong("time") < 900000) { result += existing; continue }
            try {
                val items = when (s.getString("kind")) {
                    "local" -> local(s)
                    "url" -> { val url = s.getString("location"); require(Policy.http(url)); listOf(Media(Policy.hash("$id:$url"), s.getString("name"), url, id)) }
                    "manifest" -> Policy.manifest(String(Net.bytes(s.getString("location"), 4 * 1024 * 1024)), s.getString("location"), id)
                    "webdav" -> webdav(s)
                    "drive" -> drive(s)
                    "photos" -> GoogleAmbient(context, store).media(s)
                    else -> emptyList()
                }
                val a = JSONArray(); items.forEach { a.put(it.json()) }; store.put(key, JSONObject().put("time", System.currentTimeMillis()).put("items", a)); result += items
            } catch (e: Exception) {
                if (s.getString("kind") != "photos") result += existing.filter { cacheFile(it.id).exists() }
                else if ((e as? ApiError)?.status in listOf(401, 403, 404)) { clearSource(id); store.put(key, null) }
                errors += "${s.optString("name")}: ${e.message}"
            }
        }
        if (result.isEmpty() && errors.isNotEmpty()) error(errors.joinToString("; "))
        return result.distinctBy { it.id }.take(10000)
    }
    private fun local(s: JSONObject): List<Media> {
        val root = DocumentFile.fromTreeUri(context, Uri.parse(s.getString("location"))) ?: error("Choose the folder again")
        val items = mutableListOf<Media>(); var visited = 0
        fun scan(folder: DocumentFile, depth: Int) {
            if (depth > 16 || visited++ >= 1000 || Thread.currentThread().isInterrupted) return
            for (f in folder.listFiles()) {
                if (items.size >= 10000) return
                if (f.isDirectory && s.optBoolean("recursive")) scan(f, depth + 1)
                else if (f.isFile && f.type?.startsWith("image/") == true) items += Media(Policy.hash("${s.getString("id")}:${f.uri}"), f.name ?: "Image", f.uri.toString(), s.getString("id"))
            }
        }
        scan(root, 0); return items
    }
    fun headers(media: Media): Map<String, String> {
        val sources = store.state().getJSONArray("sources")
        val source = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == media.sourceId } ?: error("Source removed")
        return when (source.getString("kind")) {
            "webdav" -> mapOf("Authorization" to "Basic " + Base64.encodeToString("${source.optString("username")}:${source.optString("password")}".toByteArray(), Base64.NO_WRAP))
            "drive" -> (DriveRelay(store).session(media.sourceId)?.optString("token")?.takeIf { it.isNotBlank() } ?: source.optString("accessToken").takeIf { it.isNotBlank() })?.let { mapOf("Authorization" to "Bearer $it") } ?: emptyMap()
            "photos" -> mapOf("Authorization" to "Bearer " + GoogleAmbient(context, store).token(media.sourceId))
            else -> emptyMap()
        }
    }
    private fun webdav(s: JSONObject): List<Media> {
        val source = s.getString("id"); val base = s.getString("location"); require(Policy.http(base))
        val auth = "Basic " + Base64.encodeToString("${s.optString("username")}:${s.optString("password")}".toByteArray(), Base64.NO_WRAP)
        require(base.startsWith("https://") || (s.optString("username").isEmpty() && s.optString("password").isEmpty())) { "WebDAV credentials require HTTPS" }
        val seen = mutableSetOf<String>(); val items = mutableListOf<Media>(); val origin = URI(base)
        fun scan(url: String, depth: Int) {
            if (depth > 16 || seen.size >= 1000 || !seen.add(url)) return
            val xml = String(Net.bytes(url, 4 * 1024 * 1024, mapOf("Authorization" to auth, "Depth" to "1"), "PROPFIND", """<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontenttype/><d:displayname/></d:prop></d:propfind>""", "application/xml"))
            val parser = Xml.newPullParser().apply { setFeature(XmlPullParser.FEATURE_PROCESS_NAMESPACES, true); setInput(StringReader(xml)) }
            var href = ""; var mime = ""; var name = ""; var folder = false
            while (parser.eventType != XmlPullParser.END_DOCUMENT) {
                if (parser.eventType == XmlPullParser.START_TAG) when (parser.name) {
                    "response" -> { href = ""; mime = ""; name = ""; folder = false }
                    "href" -> href = parser.nextText()
                    "getcontenttype" -> mime = parser.nextText()
                    "displayname" -> name = parser.nextText()
                    "collection" -> folder = true
                }
                if (parser.eventType == XmlPullParser.END_TAG && parser.name == "response" && href.isNotEmpty()) {
                    val resolved = URI(url).resolve(href)
                    if (resolved.scheme == origin.scheme && resolved.host == origin.host && resolved.port == origin.port && resolved.userInfo == null) {
                        val target = resolved.toString()
                        if (folder && s.optBoolean("recursive") && target.trimEnd('/') != url.trimEnd('/')) scan(target, depth + 1)
                        else if (!folder && (mime.startsWith("image/") || resolved.path.matches(Regex("(?i).*\\.(jpg|jpeg|png|webp|gif|bmp|heic)$"))) && items.size < 10000) items += Media(Policy.hash("$source:$target"), name.ifBlank { resolved.path.substringAfterLast('/') }, target, source)
                    }
                }
                parser.next()
            }
        }
        scan(base, 0); return items
    }
    private fun drive(s: JSONObject): List<Media> {
        val root = DriveRelay.folder(s.getString("location"))
        val key = s.optString("apiKey"); val token = s.optString("accessToken")
        val relay = DriveRelay(store).session(s.getString("id"))?.takeIf { it.optString("phase") == "ready" }
        require(relay != null || key.isNotBlank() || token.isNotBlank()) { "Connect a Drive relay, use a public API key, or supply an access token" }
        val credential = relay?.getString("token") ?: token
        val headers = if (credential.isNotBlank()) mapOf("Authorization" to "Bearer $credential") else emptyMap()
        val items = mutableListOf<Media>(); val folders = mutableSetOf<String>()
        fun scan(folder: String, depth: Int) {
            if (depth > 16 || folders.size >= 1000 || !folders.add(folder)) return
            var page = ""; var pages = 0
            do {
                val q = "'$folder' in parents and trashed = false"
                val listUrl = if (relay != null) "${relay.getString("relayUrl")}/api/drive/files?folderId=${Net.enc(folder)}&pageToken=${Net.enc(page)}" else "https://www.googleapis.com/drive/v3/files?q=${Net.enc(q)}&pageSize=1000&fields=nextPageToken,files(id,name,mimeType)&supportsAllDrives=true&includeItemsFromAllDrives=true&key=${Net.enc(key)}&pageToken=${Net.enc(page)}"
                val data = Net.json(listUrl, headers)
                val files = data.optJSONArray("files") ?: JSONArray()
                for (i in 0 until files.length()) {
                    val f = files.getJSONObject(i); val mime = f.optString("mimeType")
                    if (mime == "application/vnd.google-apps.folder" && s.optBoolean("recursive")) scan(f.getString("id"), depth + 1)
                    else if (mime.startsWith("image/") && items.size < 10000) { val id = f.getString("id"); val url = if (relay != null) "${relay.getString("relayUrl")}/api/drive/media/${Net.enc(id)}" else "https://www.googleapis.com/drive/v3/files/${Net.enc(id)}?alt=media&key=${Net.enc(key)}"; items += Media(Policy.hash("${s.getString("id")}:$id"), f.getString("name"), url, s.getString("id")) }
                }
                page = data.optString("nextPageToken")
            } while (page.isNotEmpty() && ++pages < 100 && items.size < 10000)
        }
        scan(root, 0); return items
    }
    fun cacheFile(id: String): File { require(id.matches(Regex("[a-f0-9]{64}"))); return File(dir, "$id.img") }
    fun image(media: Media): File {
        val target = cacheFile(media.id)
        val state = store.state(); val revision = store.revision(); val cacheEpoch = store.get("cacheEpoch")?.optLong("value") ?: 0L
        val sources = state.getJSONArray("sources")
        require((0 until sources.length()).any { sources.getJSONObject(it).optString("id") == media.sourceId && sources.getJSONObject(it).optBoolean("enabled", true) }) { "Source removed or disabled" }
        if (media.photos) {
            require(networkAllowed(store.state().getJSONObject("settings"))) { "Google Photos cannot be displayed offline" }
            require(store.get("google:${media.sourceId}")?.optString("phase") == "ready") { "Google Photos disconnected" }
            val cachedAt = store.get("cacheOwners")?.optJSONObject(media.id)?.optLong("createdAt") ?: 0L
            if (System.currentTimeMillis() - cachedAt > 60 * 60 * 1000) target.delete()
        }
        if (target.exists()) { target.setLastModified(System.currentTimeMillis()); return target }
        val bytes = if (media.url.startsWith("content://")) context.contentResolver.openInputStream(Uri.parse(media.url))!!.use { Net.bounded(it, 25 * 1024 * 1024) }
        else { require(networkAllowed(state.getJSONObject("settings"))) { "Network unavailable or Wi-Fi required" }; Net.bytes(media.url, 25 * 1024 * 1024, headers(media)) }
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) { "Invalid or oversized image" }
        synchronized(StateStore.lock) {
            check(store.revision() == revision && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == cacheEpoch) { "Image operation cancelled by newer state or cache clear" }
            val temp = File.createTempFile("image-", ".tmp", dir)
            try { temp.writeBytes(bytes); require(temp.renameTo(target)) { "Cannot cache image" } } finally { temp.delete() }
            val ledger = store.get("cacheOwners") ?: JSONObject(); ledger.put(media.id, JSONObject().put("source", media.sourceId).put("photos", media.photos).put("createdAt", System.currentTimeMillis())); store.put("cacheOwners", ledger)
        }
        trim(state.getJSONObject("settings").optInt("cacheLimitMb", 256).toLong() * 1024 * 1024, media.id)
        return target
    }
    fun trim(limit: Long, keep: String = "") = synchronized(StateStore.lock) {
        val files = dir.listFiles()?.filter { it.extension == "img" }?.sortedBy { it.lastModified() } ?: emptyList()
        var bytes = files.sumOf { it.length() }; val ledger = store.get("cacheOwners") ?: JSONObject()
        for (f in files) if (bytes > limit && f.nameWithoutExtension != keep) { bytes -= f.length(); f.delete(); ledger.remove(f.nameWithoutExtension) }
        store.put("cacheOwners", ledger)
    }
    fun clearSource(source: String) = synchronized(StateStore.lock) {
        val ledger = store.get("cacheOwners") ?: JSONObject()
        ledger.keys().asSequence().toList().filter { ledger.getJSONObject(it).optString("source") == source }.forEach { cacheFile(it).delete(); ledger.remove(it) }
        store.put("cacheOwners", ledger); store.put("catalog:$source", null)
    }
    fun clear() = synchronized(StateStore.lock) { dir.listFiles()?.forEach { it.delete() }; store.put("cacheOwners", null); store.put("cacheEpoch", JSONObject().put("value", (store.get("cacheEpoch")?.optLong("value") ?: 0L) + 1)) }
    fun cleanup(old: JSONObject, updated: JSONObject) {
        val a = updated.getJSONArray("sources"); val now = (0 until a.length()).map { a.getJSONObject(it) }
        val b = old.getJSONArray("sources")
        for (i in 0 until b.length()) {
            val s = b.getJSONObject(i); val id = s.getString("id"); val next = now.find { it.optString("id") == id }
            if (next == null || next.optString("location") != s.optString("location") || next.optString("kind") != s.optString("kind")) {
                clearSource(id)
                val freshPhotos = next?.optString("kind") == "photos" && store.get("google:$id")?.optString("deviceId") == next.optString("location")
                val freshDrive = next?.optString("kind") == "drive" && DriveRelay(store).session(id)?.optString("folder") == next.optString("location")
                if (!freshPhotos && !freshDrive && s.optString("kind") in listOf("photos", "drive")) GoogleAmbient(context, store).disconnect(id)
            }
            if (next != null && !next.optBoolean("enabled", true) && s.optString("kind") == "photos") clearSource(id)
        }
        val used = now.filter { it.optString("kind") == "local" }.map { it.optString("location") }.toSet()
        context.contentResolver.persistedUriPermissions.filter { it.uri.toString() !in used }.forEach { runCatching { context.contentResolver.releasePersistableUriPermission(it.uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION) } }
    }
}
