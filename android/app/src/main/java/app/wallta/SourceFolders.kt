package app.wallta

import org.json.JSONArray
import org.json.JSONObject

/** The explicit array is authoritative; legacy singleton state stays valid. */
object SourceFolders {
    /** A document ID is stable even when selected through overlapping SAF tree grants. */
    fun documentKey(location: String): String = runCatching {
        val uri = java.net.URI(location)
        val path = uri.rawPath ?: ""
        if (uri.scheme == "content" && path.contains("/document/")) "${uri.authority}:${java.net.URLDecoder.decode(path.substringAfter("/document/"), "UTF-8")}" else location
    }.getOrDefault(location)
    fun locations(source: JSONObject): List<String> {
        require(!source.has("folders") || source.optJSONArray("folders") != null) { "Invalid folders array" }
        val folders = source.optJSONArray("folders")
        if (folders == null || folders.length() == 0) return listOf(source.optString("location"))
        require(source.optString("kind") in listOf("local", "drive", "webdav")) { "This source does not support multiple folders" }
        require(folders.length() <= 50) { "Maximum 50 folders per source" }
        val result = (0 until folders.length()).map { index ->
            val entry = folders.getJSONObject(index)
            val location = entry.getString("location").trim()
            require(location.isNotEmpty() && location.length <= 8192) { "Folder location is empty or too long" }
            require(!location.startsWith("wallta-import://")) { "Imported collections use one managed location" }
            if (entry.has("name")) require(entry.getString("name").length <= 256) { "Folder name is too long" }
            location
        }
        val keys = result.map { if (source.optString("kind") == "drive") DriveRelay.folder(it) else it.trimEnd('/') }
        require(keys.distinct().size == keys.size) { "Duplicate folder location" }
        if (source.optString("kind") == "webdav") {
            val origins = result.map { location ->
                require(Policy.http(location)) { "Invalid WebDAV folder URL" }
                val uri = java.net.URI(location)
                require(uri.fragment == null && uri.rawQuery == null) { "WebDAV folder URL must not contain a query or fragment" }
                listOf(uri.scheme, uri.host.lowercase(), if (uri.port >= 0) uri.port else if (uri.scheme == "https") 443 else 80)
            }
            require(origins.distinct().size == 1) { "WebDAV folders must use the same server and protocol" }
        }
        return result
    }
    fun normalize(source: JSONObject) {
        val roots = locations(source)
        if ((source.optJSONArray("folders")?.length() ?: 0) > 0) {
            source.put("location", roots.first())
            source.getJSONArray("folders").let { folders -> roots.forEachIndexed { i, root -> folders.getJSONObject(i).put("location", root) } }
        }
    }
    fun relayScope(source: JSONObject, session: JSONObject?) {
        if (source.optString("kind") != "drive" || session?.optString("phase") != "ready") return
        if (!source.optBoolean("enabled", true) && source.optString("location").isBlank() && (source.optJSONArray("folders")?.length() ?: 0) == 0) return
        val roots = locations(source).map { DriveRelay.folder(it) }
        require(roots.size == 1 && roots.single() == session.optString("folder")) {
            "Private Drive relay is approved for one folder only. Reconnect the intended folder or use a public Drive source for multiple folders."
        }
    }
    fun fingerprint(source: JSONObject): JSONArray = JSONArray().apply { locations(source).forEach { put(it) } }
    fun contains(source: JSONObject, url: String): Boolean = locations(source).any { MediaSafety.within(url, it) }
}
