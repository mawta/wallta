package app.wallta

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.URI
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.time.Instant
import java.util.UUID

/** Every destructive operation starts from an app-owned root and native identifier. */
internal object UpscaleFiles {
    private val uuid = Regex("[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}")
    fun validId(id: String) = uuid.matches(id)
    fun plain(file: File, directory: Boolean): Boolean = !Files.isSymbolicLink(file.toPath()) && file.canonicalFile == file.absoluteFile.normalize() && if (directory) file.isDirectory else file.isFile
    fun root(file: File, create: Boolean = false): File {
        if (!file.exists() && create) check(file.mkdirs()) { "Cannot create managed storage" }
        require(plain(file, true)) { "Managed storage contains a link or is unavailable" }
        return file.canonicalFile
    }
    fun child(parent: File, id: String, create: Boolean = false): File {
        require(validId(id)) { "Invalid owned collection ID" }
        val base = root(parent, create); val child = File(base, id)
        if (!Files.exists(child.toPath(), LinkOption.NOFOLLOW_LINKS)) {
            if (create) check(child.mkdir()) { "Cannot create 4K storage" } else return child
        }
        require(plain(child, true) && child.canonicalFile.parentFile == base) { "Collection escaped managed storage" }
        return child
    }
    fun validateTree(file: File) {
        require(!Files.isSymbolicLink(file.toPath()) && file.canonicalFile == file.absoluteFile.normalize()) { "Managed storage contains a link" }
        if (file.isDirectory) file.listFiles()?.forEach { validateTree(it) } ?: error("Cannot read managed storage")
        else require(file.isFile) { "Invalid managed storage entry" }
    }
    private fun deleteValidated(file: File) {
        require(!Files.isSymbolicLink(file.toPath()) && file.canonicalFile == file.absoluteFile.normalize()) { "Managed storage contains a link" }
        if (file.isDirectory) (file.listFiles() ?: error("Cannot read managed storage")).forEach { deleteValidated(it) }
        check(file.delete()) { "Cannot delete managed storage" }
    }
    fun removeChild(parent: File, id: String) {
        require(validId(id)) { "Invalid owned collection ID" }
        if (!parent.exists()) return
        val file = child(parent, id)
        if (!file.exists()) return
        validateTree(file); deleteValidated(file)
    }
    fun clearCacheFolder(cache: File, name: String) {
        require(name in listOf("media", "visual", "upscale", "upscale-results"))
        val base = root(cache); val target = File(base, name)
        if (!Files.exists(target.toPath(), LinkOption.NOFOLLOW_LINKS)) return
        require(plain(target, true) && target.canonicalFile.parentFile == base)
        validateTree(target); deleteValidated(target)
    }
    fun atomicWrite(target: File, bytes: ByteArray) {
        require(target.name.isNotEmpty() && target.path == target.absoluteFile.normalize().path) { "Invalid atomic file path" }
        val parent = root(target.parentFile ?: error("Missing output folder"))
        require(!Files.exists(target.toPath(), LinkOption.NOFOLLOW_LINKS) || plain(target, false)) { "Output contains a link" }
        val temp = File(parent, ".${UUID.randomUUID()}.pending")
        check(temp.createNewFile()) { "Cannot stage output" }
        try {
            temp.outputStream().use { it.write(bytes); it.fd.sync() }
            Files.move(temp.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } finally { temp.delete() }
    }
}

internal class UpscaleCollections(private val context: Context, private val store: StateStore) {
    data class Owned(val id: String, val folder: File, val source: JSONObject, val manifest: JSONObject, val legacy: Boolean)
    // Android's trusted Context roots may use /data/user/0 or /data/data aliases.
    // Resolve those once; every owned descendant still rejects links.
    private val filesRoot = context.filesDir.canonicalFile
    private val cacheRoot = context.cacheDir.canonicalFile
    private val root get() = File(filesRoot, "upscaled")
    companion object {
        fun sourceId(job: String) = UUID.nameUUIDFromBytes("wallta-upscale:$job".toByteArray()).toString()
        fun location(job: String): String { require(UpscaleFiles.validId(job)); return "wallta-upscale://$job" }
        fun locationId(location: String): String? = location.removePrefix("wallta-upscale://").takeIf { location == "wallta-upscale://$it" && UpscaleFiles.validId(it) }
        fun legacyJob(source: JSONObject): String? = runCatching {
            if (source.optString("kind") != "local") return null
            val uri = URI(source.optString("location")); val job = uri.path.removePrefix("/")
            if (uri.scheme != "wallta-import" || !UpscaleFiles.validId(job) || uri.path != "/$job" || uri.query != null || uri.fragment != null) return null
            val id = UUID.nameUUIDFromBytes("wallta-4k:$job".toByteArray()).toString()
            job.takeIf { source.optString("id") == id && uri.host == Policy.hash(id) }
        }.getOrNull()
    }
    fun isManaged(source: JSONObject) = source.optString("kind") == "local" && (locationId(source.optString("location")) != null || legacyJob(source) != null)
    fun append(job: String, output: UpscaleManager.Output, index: Int, expected: Long): UpscaleManager.Publication = synchronized(StateStore.lock) {
        check(store.revision() == expected && !Thread.currentThread().isInterrupted) { "Sources changed. Start 4K processing again." }
        val state = store.state(); val sources = state.getJSONArray("sources")
        val id = sourceId(job); val location = location(job)
        val existing = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == id }
        if (existing == null) require(sources.length() < 100) { "Maximum 100 sources" }
        else check(existing.optString("location") == location && !existing.optBoolean("enabled", true)) { "Output collection changed" }
        val folder = UpscaleFiles.child(root, job, true)
        check(folder.usableSpace >= output.file.length() + 16L * 1024 * 1024) { "Not enough free storage for 4K collection" }
        val manifestFile = File(folder, "collection.json")
        val manifest = if (manifestFile.exists()) readManifest(folder, job) else JSONObject().put("id", job).put("createdAt", Instant.now().toString()).put("model", JSONObject.NULL).put("items", JSONArray())
        val originalName = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == output.sourceId }?.optString("name")
        val scope = if (output.multipleSources) "Tất cả nguồn" else originalName?.filter { !it.isISOControl() }?.trim()?.take(48)?.ifBlank { null } ?: "Ảnh gốc"
        val timestamp = manifest.getString("createdAt").take(19).replace('T', ' ')
        val source = existing ?: JSONObject().put("id", id).put("name", "Ảnh 4K • $scope • Fast • $timestamp").put("kind", "local").put("location", location).put("recursive", false).put("enabled", false)
        val name = "${Policy.hash("$job:$index")}.img"
        val items = manifest.getJSONArray("items")
        check(index == items.length()) { "Collection append is out of order" }
        UpscaleFiles.atomicWrite(File(folder, name), output.file.readBytes())
        items.put(JSONObject().put("file", name).put("name", output.name))
        manifest.put("source", source)
        UpscaleFiles.atomicWrite(manifestFile, manifest.toString().toByteArray())
        // Only the manager's own append is rebased. An external save before this gate
        // fails the revision check; a save after return is seen before the next image.
        check(!Thread.currentThread().isInterrupted) { "4K processing cancelled" }
        if (existing == null) sources.put(source)
        store.put("catalog:$id", null)
        store.save(state)
        UpscaleManager.Publication(JSONObject(source.toString()), store.revision())
    }
    private fun readManifest(folder: File, id: String): JSONObject {
        val file = File(folder, "collection.json")
        require(UpscaleFiles.plain(file, false) && file.length() <= 8L * 1024 * 1024) { "Invalid collection metadata" }
        val value = JSONObject(file.readText())
        require(value.optString("id") == id && value.getJSONObject("source").optString("location") == location(id) && value.getJSONObject("source").optString("id") == sourceId(id)) { "Unrecognized owned collection" }
        return value
    }
    private fun nativeCollections(): List<Owned> {
        if (!root.exists()) return emptyList()
        val base = UpscaleFiles.root(root)
        return (base.listFiles() ?: error("Cannot read 4K collections")).mapNotNull { folder ->
            if (!UpscaleFiles.validId(folder.name)) return@mapNotNull null
            runCatching {
                val safe = UpscaleFiles.child(base, folder.name)
                val manifest = readManifest(safe, folder.name)
                Owned(folder.name, safe, manifest.getJSONObject("source"), manifest, false)
            }.getOrNull()
        }
    }
    private fun legacyFolder(source: JSONObject, job: String): File {
        val imports = UpscaleFiles.root(File(filesRoot, "imports"))
        val parent = File(imports, Policy.hash(source.getString("id")))
        require(UpscaleFiles.plain(parent, true) && parent.canonicalFile.parentFile == imports) { "Unsafe legacy collection" }
        return UpscaleFiles.child(parent, job)
    }
    fun all(): List<Owned> = synchronized(StateStore.lock) {
        val result = nativeCollections().toMutableList()
        val sources = store.state().getJSONArray("sources")
        for (i in 0 until sources.length()) {
            val source = sources.getJSONObject(i); val job = legacyJob(source) ?: continue
            runCatching {
                val session = store.session("import", source.getString("id"), true) ?: return@runCatching
                require(session.optString("location") == source.getString("location"))
                result += Owned("legacy:${source.getString("id")}", legacyFolder(source, job), source, session, true)
            }
        }
        // Older releases used a deterministic source ID and import parent. Recover
        // those exact folders even after their source/session was removed; ordinary
        // Photos/LAN imports do not satisfy this identity and remain untouched.
        val imports = File(filesRoot, "imports")
        if (imports.exists()) {
            val base = UpscaleFiles.root(imports)
            for (parent in base.listFiles().orEmpty()) {
                if (!parent.name.matches(Regex("[a-f0-9]{64}")) || !UpscaleFiles.plain(parent, true)) continue
                for (folder in parent.listFiles().orEmpty()) {
                    val job = folder.name
                    if (!UpscaleFiles.validId(job)) continue
                    val sourceId = UUID.nameUUIDFromBytes("wallta-4k:$job".toByteArray()).toString()
                    val ownedId = "legacy:$sourceId"
                    if (parent.name != Policy.hash(sourceId) || result.any { it.id == ownedId }) continue
                    runCatching {
                        val safe = UpscaleFiles.child(parent, job)
                        val location = "wallta-import://${parent.name}/$job"
                        val source = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == sourceId && it.optString("location") == location }
                            ?: JSONObject().put("id", sourceId).put("name", "Ảnh 4K").put("kind", "local").put("location", location).put("enabled", false).put("recursive", false)
                        val items = JSONArray()
                        safe.listFiles().orEmpty().filter { it.name.matches(Regex("[a-f0-9]{64}\\.img")) && UpscaleFiles.plain(it, false) }.take(10000).forEach { file ->
                            items.put(JSONObject().put("file", file.name).put("name", "Ảnh 4K"))
                        }
                        result += Owned(ownedId, safe, source, JSONObject().put("items", items), true)
                    }
                }
            }
        }
        result
    }
    private fun entries(owned: Owned): List<Pair<File, String>> {
        val items = owned.manifest.optJSONArray("items") ?: JSONArray()
        require(items.length() <= 10000) { "Collection is oversized" }
        return (0 until items.length()).mapNotNull { i ->
            val item = items.getJSONObject(i); val name = item.optString("file")
            if (!name.matches(Regex("[a-f0-9]{64}\\.img"))) return@mapNotNull null
            val file = File(owned.folder, name)
            if (UpscaleFiles.plain(file, false)) file to item.optString("name", "Ảnh 4K") else null
        }
    }
    fun media(source: JSONObject): List<Media> {
        val job = locationId(source.getString("location")) ?: error("Invalid 4K location")
        val folder = UpscaleFiles.child(root, job)
        val owned = Owned(job, folder, source, readManifest(folder, job), false)
        require(source.optString("id") == owned.manifest.getJSONObject("source").optString("id"))
        return entries(owned).map { (file, name) -> Media(Policy.hash("${source.getString("id")}:${source.getString("location")}:${file.name}"), name, "${source.getString("location")}/${file.name}", source.getString("id")) }
    }
    fun image(url: String): File {
        val slash = url.lastIndexOf('/'); val job = locationId(url.substring(0, slash)) ?: error("Invalid 4K image location")
        val name = url.substring(slash + 1); require(name.matches(Regex("[a-f0-9]{64}\\.img")))
        return File(UpscaleFiles.child(root, job), name).also { require(UpscaleFiles.plain(it, false)) }
    }
    fun storage(): JSONObject = synchronized(StateStore.lock) {
        val sources = store.state().getJSONArray("sources")
        val locations = (0 until sources.length()).associate { sources.getJSONObject(it).optString("location") to sources.getJSONObject(it).optString("id") }
        val collections = JSONArray()
        all().forEach { owned ->
            val entries = entries(owned)
            collections.put(JSONObject().put("id", owned.id).put("name", owned.source.optString("name", "Ảnh 4K") + if (owned.legacy) " (legacy)" else "")
                .put("model", JSONObject.NULL).put("createdAt", if (owned.legacy) JSONObject.NULL else owned.manifest.opt("createdAt") ?: JSONObject.NULL)
                .put("count", entries.size).put("bytes", entries.sumOf { it.first.length() }).put("sourceId", locations[owned.source.optString("location")] ?: JSONObject.NULL))
        }
        val bytes = listOf("media", "visual", "upscale", "upscale-results").sumOf { name ->
            val dir = File(cacheRoot, name)
            if (!dir.exists()) 0L else { require(UpscaleFiles.plain(dir, true)); UpscaleFiles.validateTree(dir); dir.walkTopDown().filter { it.isFile }.sumOf { it.length() } }
        }
        JSONObject().put("cacheBytes", bytes).put("collections", collections)
    }
    fun clearCache(): JSONObject = synchronized(StateStore.lock) {
        // Preflight all targets before invalidating state or deleting any cache.
        listOf("media", "visual", "upscale", "upscale-results").map { File(cacheRoot, it) }.filter { it.exists() }.forEach { UpscaleFiles.validateTree(it) }
        store.invalidateCache()
        listOf("media", "visual", "upscale", "upscale-results").forEach { UpscaleFiles.clearCacheFolder(cacheRoot, it) }
        store.put("cacheOwners", null)
        storage()
    }
    fun delete(id: String): JSONObject = synchronized(StateStore.lock) {
        require(UpscaleFiles.validId(id) || id.startsWith("legacy:") && UpscaleFiles.validId(id.removePrefix("legacy:"))) { "Invalid collection ID" }
        val owned = all().find { it.id == id } ?: error("Owned collection no longer available")
        UpscaleFiles.validateTree(owned.folder)
        // State/current/cache references are removed in one fresh encrypted-root write.
        // No WallpaperManager operation is issued: the rendered system wallpaper stays.
        store.removeUpscaleCollection(owned.source.getString("location"))
        if (owned.legacy) UpscaleFiles.removeChild(owned.folder.parentFile!!, owned.folder.name)
        else UpscaleFiles.removeChild(root, owned.id)
        storage()
    }
}
