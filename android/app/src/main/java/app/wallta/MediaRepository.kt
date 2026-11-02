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
    private val dir = File(context.cacheDir.canonicalFile, "media").apply { mkdirs() }
    /** Resolve only original enabled local media; never export a remote image or a cached derivative. */
    fun lanExports(ids: List<String>): List<LanTransferServer.Export> {
        require(ids.size <= 50 && ids.distinct().size == ids.size && ids.all { it.matches(Regex("[a-f0-9]{64}")) }) { "Invalid local image selection" }
        if (ids.isEmpty()) return emptyList()
        val snapshot = synchronized(StateStore.lock) { store.state() to store.revision() }
        val sources = snapshot.first.getJSONArray("sources")
        val selected = mutableMapOf<String, Media>()
        for (i in 0 until sources.length()) {
            val source = sources.getJSONObject(i)
            if (source.optString("kind") == "local" && source.optBoolean("enabled", true)) {
                local(source).filter { it.id in ids }.forEach { selected[it.id] = it }
            }
        }
        return ids.map { id ->
            val media = selected[id] ?: error("Selected image is no longer available in an enabled local source")
            LanTransferServer.Export(id, media.name) {
                synchronized(StateStore.lock) { check(store.revision() == snapshot.second) { "Local sources changed. Start the transfer again." } }
                if (media.url.startsWith("wallta-upscale://")) {
                    UpscaleCollections(context, store).image(media.url).inputStream()
                } else if (media.url.startsWith("wallta-import://")) {
                    val slash = media.url.lastIndexOf('/'); val name = media.url.substring(slash + 1)
                    require(name.matches(Regex("[a-f0-9]{64}\\.img")))
                    File(PhotoImporter.directory(context, media.url.substring(0, slash)), name).inputStream()
                } else {
                    val uri = Uri.parse(media.url)
                    require(uri.scheme == "content") { "Selected file is not local" }
                    context.contentResolver.openInputStream(uri) ?: error("Cannot read selected local image")
                }
            }
        }
    }
    fun networkAllowed(settings: JSONObject): Boolean {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
        return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && (!settings.optBoolean("wifiOnly") || caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI))
    }
    fun list(force: Boolean = false, sourceIds: Set<String>? = null, originalOnly: Boolean = false, strict: Boolean = false, includeDisabled: Boolean = false): List<Media> {
        // A single generation owns the entire list operation, including later sources.
        val snapshot = synchronized(StateStore.lock) { Triple(store.state(), store.revision(), store.get("cacheEpoch")?.optLong("value") ?: 0L) }
        val state = snapshot.first; val revision = snapshot.second; val epoch = snapshot.third
        val settings = state.getJSONObject("settings"); val sources = state.getJSONArray("sources")
        fun ensureCurrent(source: JSONObject? = null) = synchronized(StateStore.lock) {
            check(store.revision() == revision && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == epoch && !Thread.currentThread().isInterrupted) { "Catalog operation cancelled by newer state or source removal" }
            if (source != null) {
                val saved = store.state().getJSONArray("sources")
                val current = (0 until saved.length()).map { saved.getJSONObject(it) }.find { it.optString("id") == source.getString("id") }
                check(current != null && MediaSafety.sourceConfig(current) == MediaSafety.sourceConfig(source)) { "Catalog source was removed or edited" }
            }
        }
        val result = mutableListOf<Media>(); val errors = mutableListOf<String>()
        for (i in 0 until sources.length()) {
            ensureCurrent()
            val s = sources.getJSONObject(i); if (!includeDisabled && !s.optBoolean("enabled", true) || sourceIds != null && s.optString("id") !in sourceIds || originalOnly && UpscaleCollections(context, store).isManaged(s)) continue
            ensureCurrent(s)
            val id = s.getString("id"); val key = "catalog:$id"; val cached = store.get(key)
            val existing = cached?.optJSONArray("items")?.let { a -> (0 until a.length()).map { Media.from(a.getJSONObject(it)) } } ?: emptyList()
            val online = networkAllowed(settings)
            if (s.getString("kind") != "local" && !online) {
                check(!strict) { "Network unavailable or Wi-Fi required for this source" }
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
                val a = JSONArray(); items.forEach { a.put(it.json()) }
                synchronized(StateStore.lock) {
                    ensureCurrent(s)
                    store.put(key, JSONObject().put("time", System.currentTimeMillis()).put("items", a))
                }
                result += items
            } catch (e: Exception) {
                // An invalidated operation must never continue with its stale later sources.
                ensureCurrent(s)
                if (strict) throw e
                if (s.getString("kind") != "photos") result += existing.filter { cacheFile(it.id).exists() }
                else if ((e as? ApiError)?.status in listOf(401, 403, 404)) { clearSource(id); store.put(key, null) }
                errors += "${s.optString("name")}: ${e.message}"
            }
        }
        ensureCurrent()
        if (result.isEmpty() && errors.isNotEmpty()) error(errors.joinToString("; "))
        return result.distinctBy { it.id }.take(10000)
    }
    private fun local(s: JSONObject): List<Media> {
        if (s.getString("location").startsWith("wallta-upscale://")) return UpscaleCollections(context, store).media(s)
        if (s.getString("location").startsWith("wallta-import://")) {
            val folder = PhotoImporter.directory(context, s.getString("location")); val saved = store.session("import", s.getString("id"), true) ?: error("Imported photos are no longer available")
            require(saved.optString("location") == s.getString("location")); val a = saved.optJSONArray("items") ?: JSONArray()
            return (0 until a.length()).mapNotNull { i -> val entry = a.getJSONObject(i); val name = entry.getString("file"); if (!name.matches(Regex("[a-f0-9]{64}\\.img")) || !File(folder, name).isFile) null else Media(Policy.hash("${s.getString("id")}:${s.getString("location")}:$name"), entry.optString("name", "Photo"), "${s.getString("location")}/$name", s.getString("id")) }
        }
        val items = mutableListOf<Media>(); val folders = mutableSetOf<String>(); var visited = 0; var entries = 0
        val deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(60)
        fun scan(folder: DocumentFile, depth: Int) {
            if (depth > 16 || visited >= 1000 || entries >= 100000 || System.nanoTime() >= deadline || Thread.currentThread().isInterrupted || !folders.add(SourceFolders.documentKey(folder.uri.toString()))) return
            visited++
            for (f in folder.listFiles()) {
                if (items.size >= 10000 || entries++ >= 100000 || System.nanoTime() >= deadline || Thread.currentThread().isInterrupted) return
                if (f.isDirectory && s.optBoolean("recursive")) scan(f, depth + 1)
                else if (f.isFile && f.type?.startsWith("image/") == true) items += Media(Policy.hash("${s.getString("id")}:${SourceFolders.documentKey(f.uri.toString())}"), f.name ?: "Image", f.uri.toString(), s.getString("id"))
            }
        }
        SourceFolders.locations(s).forEach { location ->
            val root = DocumentFile.fromTreeUri(context, Uri.parse(location)) ?: error("Choose the folder again")
            check(root.exists() && root.isDirectory && root.canRead()) { "Local or USB folder is unavailable. Reconnect storage or choose the folder again." }
            scan(root, 0)
        }
        return items.distinctBy { it.id }
    }
    fun headers(media: Media): Map<String, String> {
        val sources = store.state().getJSONArray("sources")
        val source = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == media.sourceId } ?: error("Source removed")
        return when (source.getString("kind")) {
            "webdav" -> { require(SourceFolders.contains(source, media.url)) { "Image is outside the active WebDAV origin/folder" }; mapOf("Authorization" to "Basic " + Base64.encodeToString("${source.optString("username")}:${source.optString("password")}".toByteArray(), Base64.NO_WRAP)) }
            "drive" -> {
                val relay = DriveRelay(store).session(media.sourceId)?.takeIf { it.optString("phase") == "ready" }
                val base = if (relay != null) "${relay.getString("relayUrl")}/api/drive/media/" else "https://www.googleapis.com/drive/v3/files/"
                require(MediaSafety.within(media.url, base)) { "Image is outside the active Drive provider" }
                (relay?.optString("token")?.takeIf { it.isNotBlank() } ?: source.optString("accessToken").takeIf { it.isNotBlank() })?.let { mapOf("Authorization" to "Bearer $it") } ?: emptyMap()
            }
            "photos" -> { require(MediaSafety.photosUrl(media.url)) { "Photos image must use Google's media origin" }; mapOf("Authorization" to "Bearer " + (source.optString("accessToken").takeIf { it.isNotBlank() } ?: GoogleAmbient(context, store).token(media.sourceId))) }
            else -> emptyMap()
        }
    }
    private fun webdav(s: JSONObject): List<Media> {
        val source = s.getString("id"); val roots = SourceFolders.locations(s); roots.forEach { require(Policy.http(it)) }
        val auth = "Basic " + Base64.encodeToString("${s.optString("username")}:${s.optString("password")}".toByteArray(), Base64.NO_WRAP)
        require(roots.all { it.startsWith("https://") } || (s.optString("username").isEmpty() && s.optString("password").isEmpty())) { "WebDAV credentials require HTTPS" }
        val seen = mutableSetOf<String>(); val items = linkedMapOf<String, Media>()
        val deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(60)
        fun scan(url: String, depth: Int) {
            if (depth > 16 || seen.size >= 1000 || items.size >= 10000 || System.nanoTime() >= deadline || Thread.currentThread().isInterrupted || !SourceFolders.contains(s, url) || !seen.add(url.trimEnd('/'))) return
            val xml = String(Net.bytes(url, 4 * 1024 * 1024, mapOf("Authorization" to auth, "Depth" to "1"), "PROPFIND", """<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontenttype/><d:displayname/></d:prop></d:propfind>""", "application/xml"))
            val parser = Xml.newPullParser().apply { setFeature(XmlPullParser.FEATURE_PROCESS_NAMESPACES, true); setInput(StringReader(xml)) }
            var href = ""; var mime = ""; var name = ""; var folder = false
            while (parser.eventType != XmlPullParser.END_DOCUMENT) {
                if (System.nanoTime() >= deadline || Thread.currentThread().isInterrupted) return
                if (parser.eventType == XmlPullParser.START_TAG) when (parser.name) {
                    "response" -> { href = ""; mime = ""; name = ""; folder = false }
                    "href" -> href = parser.nextText()
                    "getcontenttype" -> mime = parser.nextText()
                    "displayname" -> name = parser.nextText()
                    "collection" -> folder = true
                }
                if (parser.eventType == XmlPullParser.END_TAG && parser.name == "response" && href.isNotEmpty()) {
                    val resolved = URI(url).resolve(href)
                    if (SourceFolders.contains(s, resolved.toString())) {
                        val target = resolved.toString()
                        if (folder && s.optBoolean("recursive") && target.trimEnd('/') != url.trimEnd('/')) scan(target, depth + 1)
                        else if (!folder && (mime.startsWith("image/") || resolved.path.matches(Regex("(?i).*\\.(jpg|jpeg|png|webp|gif|bmp|heic)$"))) && items.size < 10000) items[target] = Media(Policy.hash("$source:$target"), name.ifBlank { resolved.path.substringAfterLast('/') }, target, source)
                    }
                }
                parser.next()
            }
        }
        roots.forEach { scan(it, 0) }; return items.values.toList()
    }
    private fun drive(s: JSONObject): List<Media> {
        val roots = SourceFolders.locations(s).map { DriveRelay.folder(it) }
        val key = s.optString("apiKey"); val token = s.optString("accessToken")
        val relay = DriveRelay(store).session(s.getString("id"))?.takeIf { it.optString("phase") == "ready" }
        require(relay != null || key.isNotBlank() || token.isNotBlank()) { "Connect a Drive relay, use a public API key, or supply an access token" }
        SourceFolders.relayScope(s, relay)
        val credential = relay?.getString("token") ?: token
        val headers = if (credential.isNotBlank()) mapOf("Authorization" to "Bearer $credential") else emptyMap()
        val items = linkedMapOf<String, Media>(); val folders = mutableSetOf<String>(); var requests = 0
        val deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(60)
        fun scan(folder: String, depth: Int) {
            if (depth > 16 || folders.size >= 1000 || items.size >= 10000 || requests >= 100 || System.nanoTime() >= deadline || Thread.currentThread().isInterrupted || !folders.add(folder)) return
            var page = ""; var pages = 0
            do {
                val q = "'$folder' in parents and trashed = false"
                val listUrl = if (relay != null) "${relay.getString("relayUrl")}/api/drive/files?folderId=${Net.enc(folder)}&pageToken=${Net.enc(page)}" else "https://www.googleapis.com/drive/v3/files?q=${Net.enc(q)}&pageSize=1000&fields=nextPageToken,files(id,name,mimeType)&supportsAllDrives=true&includeItemsFromAllDrives=true&key=${Net.enc(key)}&pageToken=${Net.enc(page)}"
                if (requests++ >= 100 || System.nanoTime() >= deadline || Thread.currentThread().isInterrupted) return
                val data = Net.json(listUrl, headers)
                val files = data.optJSONArray("files") ?: JSONArray()
                for (i in 0 until files.length()) {
                    val f = files.getJSONObject(i); val mime = f.optString("mimeType")
                    if (mime == "application/vnd.google-apps.folder" && s.optBoolean("recursive")) scan(f.getString("id"), depth + 1)
                    else if (mime.startsWith("image/") && items.size < 10000) { val id = f.getString("id"); val url = if (relay != null) "${relay.getString("relayUrl")}/api/drive/media/${Net.enc(id)}" else "https://www.googleapis.com/drive/v3/files/${Net.enc(id)}?alt=media&key=${Net.enc(key)}"; items[id] = Media(Policy.hash("${s.getString("id")}:$id"), f.getString("name"), url, s.getString("id")) }
                }
                page = data.optString("nextPageToken")
            } while (page.isNotEmpty() && ++pages < 100 && items.size < 10000)
        }
        roots.forEach { scan(it, 0) }; return items.values.toList()
    }
    fun cacheFile(id: String): File { require(id.matches(Regex("[a-f0-9]{64}"))); return File(dir, "$id.img") }
    /** Explicit viewing may resolve a disabled source, without changing its rotation state. */
    fun photoPreview(id: String, sourceId: String): JSONObject {
        require(id.matches(Regex("[a-f0-9]{64}")) && sourceId.length in 1..128) { "Invalid photo selection" }
        val snapshot = synchronized(StateStore.lock) { store.revision() to (store.get("cacheEpoch")?.optLong("value") ?: 0L) }
        fun resolve(mediaId: String, source: String): Media = list(force = true, sourceIds = setOf(source), strict = true, includeDisabled = true)
            .find { it.id == mediaId && it.sourceId == source } ?: error("Image no longer available")
        // Trim once the pair is complete; resolving the original must not evict its output.
        fun prepare(media: Media) = PhotoPreviewCache.prepare(context, store, media, image(media, includeDisabled = true, trimCache = false), snapshot)
        val media = resolve(id, sourceId)
        val result = prepare(media)
        var beforeId: String? = null
        val sources = store.state().getJSONArray("sources")
        val source = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == sourceId } ?: error("Source removed")
        val collections = UpscaleCollections(context, store)
        if (collections.isManaged(source)) {
            val before = runCatching {
                val original = collections.original(media) ?: error("Original association unavailable")
                val originalMedia = resolve(original.first, original.second)
                val originalSource = store.state().getJSONArray("sources").let { a -> (0 until a.length()).map { a.getJSONObject(it) }.first { it.optString("id") == original.second } }
                require(!collections.isManaged(originalSource)) { "Original association is not an original photo" }
                prepare(originalMedia).also { beforeId = originalMedia.id }
            }.getOrNull()
            if (before != null) result.put("before", before) else result.put("comparisonUnavailable", true)
        }
        synchronized(StateStore.lock) {
            check(!Thread.currentThread().isInterrupted && store.revision() == snapshot.first && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == snapshot.second) { "Preview operation cancelled" }
            val limit = store.state().getJSONObject("settings").optInt("cacheLimitMb", 256).toLong() * 1024 * 1024
            val originalId = beforeId
            if (originalId != null && cacheBytes(id) + cacheBytes(originalId) > limit) {
                result.remove("before"); result.put("comparisonUnavailable", true); beforeId = null
            }
            trim(limit, id, setOfNotNull(beforeId))
        }
        return result
    }
    fun visual(media: Media): File {
        val snapshot = synchronized(StateStore.lock) { store.revision() to (store.get("cacheEpoch")?.optLong("value") ?: 0L) }
        return VisualCache.prepare(context, store, media, image(media), snapshot)
    }
    fun image(media: Media, includeDisabled: Boolean = false, trimCache: Boolean = true): File {
        val target = cacheFile(media.id)
        val state = store.state(); val revision = store.revision(); val cacheEpoch = store.get("cacheEpoch")?.optLong("value") ?: 0L
        val sources = state.getJSONArray("sources")
        require((0 until sources.length()).any { sources.getJSONObject(it).optString("id") == media.sourceId && (includeDisabled || sources.getJSONObject(it).optBoolean("enabled", true)) }) { "Source removed or disabled" }
        if (media.photos) {
            require(networkAllowed(store.state().getJSONObject("settings"))) { "Google Photos cannot be displayed offline" }
            val manual = (0 until sources.length()).map { sources.getJSONObject(it) }.any { it.optString("id") == media.sourceId && it.optString("kind") == "photos" && it.optString("accessToken").isNotBlank() }
            require(manual || store.get("google:${media.sourceId}")?.optString("phase") == "ready") { "Google Photos disconnected" }
            val cachedAt = store.get("cacheOwners")?.optJSONObject(media.id)?.optLong("createdAt") ?: 0L
            if (System.currentTimeMillis() - cachedAt > 60 * 60 * 1000) target.delete()
        }
        if (target.exists()) { target.setLastModified(System.currentTimeMillis()); return target }
        val bytes = if (media.url.startsWith("wallta-upscale://")) {
            UpscaleCollections(context, store).image(media.url).inputStream().use { Net.bounded(it, 64 * 1024 * 1024) }
        } else if (media.url.startsWith("wallta-import://")) {
            val slash = media.url.lastIndexOf('/'); val location = media.url.substring(0, slash); val name = media.url.substring(slash + 1)
            require(name.matches(Regex("[a-f0-9]{64}\\.img")))
            File(PhotoImporter.directory(context, location), name).inputStream().use { Net.bounded(it, 25 * 1024 * 1024) }
        } else if (media.url.startsWith("content://")) context.contentResolver.openInputStream(Uri.parse(media.url))!!.use { Net.bounded(it, 25 * 1024 * 1024) }
        else {
            require(networkAllowed(state.getJSONObject("settings"))) { "Network unavailable or Wi-Fi required" }
            val source = (0 until sources.length()).map { sources.getJSONObject(it) }.first { it.optString("id") == media.sourceId }
            val scope: ((String) -> Boolean)? = if (source.optString("kind") == "webdav") ({ url -> SourceFolders.contains(source, url) }) else null
            Net.bytes(media.url, 25 * 1024 * 1024, headers(media), allowedUrl = scope)
        }
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        require(bounds.outWidth > 0 && bounds.outHeight > 0 && bounds.outWidth.toLong() * bounds.outHeight <= 120_000_000) { "Invalid or oversized image" }
        synchronized(StateStore.lock) {
            check(!Thread.currentThread().isInterrupted && store.revision() == revision && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == cacheEpoch) { "Image operation cancelled by newer state or cache clear" }
            UpscaleFiles.root(dir, create = true)
            val temp = File.createTempFile("image-", ".tmp", dir)
            try { temp.writeBytes(bytes); require(temp.renameTo(target)) { "Cannot cache image" } } finally { temp.delete() }
            VisualCache.file(context, media.id).delete(); PhotoPreviewCache.remove(context, media.id)
            val ledger = store.get("cacheOwners") ?: JSONObject(); ledger.put(media.id, JSONObject().put("source", media.sourceId).put("photos", media.photos).put("createdAt", System.currentTimeMillis())); store.put("cacheOwners", ledger)
        }
        if (trimCache) trim(state.getJSONObject("settings").optInt("cacheLimitMb", 256).toLong() * 1024 * 1024, media.id)
        return target
    }
    private fun cacheBytes(id: String) = cacheFile(id).length() + VisualCache.file(context, id).length() + PhotoPreviewCache.length(context, id)
    fun trim(limit: Long, keep: String = "", alsoKeep: Set<String> = emptySet()) = synchronized(StateStore.lock) {
        val files = dir.listFiles()?.filter { it.extension == "img" }?.sortedBy { it.lastModified() } ?: emptyList()
        var bytes = files.sumOf { cacheBytes(it.nameWithoutExtension) }; val ledger = store.get("cacheOwners") ?: JSONObject()
        for (f in files) if (bytes > limit && f.nameWithoutExtension != keep && f.nameWithoutExtension !in alsoKeep) { val visual = VisualCache.file(context, f.nameWithoutExtension); bytes -= cacheBytes(f.nameWithoutExtension); f.delete(); visual.delete(); PhotoPreviewCache.remove(context, f.nameWithoutExtension); ledger.remove(f.nameWithoutExtension) }
        store.put("cacheOwners", ledger)
    }
    fun clearSource(source: String) = synchronized(StateStore.lock) {
        store.invalidateCache()
        val ledger = store.get("cacheOwners") ?: JSONObject()
        ledger.keys().asSequence().toList().filter { ledger.getJSONObject(it).optString("source") == source }.forEach { cacheFile(it).delete(); VisualCache.file(context, it).delete(); PhotoPreviewCache.remove(context, it); ledger.remove(it) }
        store.put("cacheOwners", ledger); store.put("catalog:$source", null)
    }
    fun clear() = synchronized(StateStore.lock) { store.invalidateCache(); dir.listFiles()?.forEach { it.delete() }; File(context.cacheDir, "visual").listFiles()?.forEach { it.delete() }; File(context.cacheDir, "photo-preview").listFiles()?.forEach { it.delete() }; store.put("cacheOwners", null) }
    fun releaseUnusedGrants() {
        val a = store.state().getJSONArray("sources")
        val used = (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") == "local" }.flatMap { SourceFolders.locations(it) }.toSet()
        context.contentResolver.persistedUriPermissions.filter { it.uri.toString() !in used }.forEach { runCatching { context.contentResolver.releasePersistableUriPermission(it.uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION) } }
    }
    fun cleanup(old: JSONObject, updated: JSONObject) {
        val a = updated.getJSONArray("sources"); val now = (0 until a.length()).map { a.getJSONObject(it) }
        val b = old.getJSONArray("sources")
        for (i in 0 until b.length()) {
            val s = b.getJSONObject(i); val id = s.getString("id"); val next = now.find { it.optString("id") == id }
            if (next == null || next.optString("location") != s.optString("location") || next.optString("kind") != s.optString("kind")) {
                clearSource(id)
                // Generated legacy images are durable collections. Disconnecting a
                // source must leave their files for the explicit folder-delete flow.
                if (s.optString("location").startsWith("wallta-import://") && UpscaleCollections.legacyJob(s) == null) PhotoImporter(context, store).remove(id, s.getString("location"))
                val freshPhotos = next?.optString("kind") == "photos" && store.get("google:$id")?.optString("deviceId") == next.optString("location")
                val relay = DriveRelay(store).session(id)
                val retainedDriveConnection = s.optString("kind") == "drive" && next?.optString("kind") == "drive" &&
                    !next.optBoolean("enabled", true) && next.optString("location").isBlank() && (next.optJSONArray("folders")?.length() ?: 0) == 0 &&
                    relay?.optString("phase") == "ready" && relay.optString("token").isNotBlank()
                val freshDrive = next?.optString("kind") == "drive" && (relay?.optString("folder") == next.optString("location") || retainedDriveConnection)
                if (!freshPhotos && !freshDrive && s.optString("kind") in listOf("photos", "drive")) GoogleAmbient(context, store).disconnect(id)
            }
            if (next != null && !next.optBoolean("enabled", true) && s.optString("kind") == "photos") clearSource(id)
        }
        releaseUnusedGrants()
    }
}
