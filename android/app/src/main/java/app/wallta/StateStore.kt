package app.wallta

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** All credentials and OAuth sessions remain in an AES/GCM file backed by Android Keystore. */
class StateStore(private val context: Context) {
    companion object {
        val lock = Any()
        private val secretKeys = listOf("username", "password", "apiKey", "accessToken")
        private const val alias = "wallta-state-v1"
    }
    private val file = AtomicFile(File(context.filesDir, "state.enc"))
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        return (ks.getKey(alias, null) as? SecretKey) ?: KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    private fun read(): JSONObject {
        if (!file.baseFile.exists()) return JSONObject()
        val bytes = file.readFully()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
        return JSONObject(String(cipher.doFinal(bytes.copyOfRange(12, bytes.size)), Charsets.UTF_8))
    }
    private fun write(root: JSONObject) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val stream = file.startWrite()
        try { stream.write(cipher.iv + cipher.doFinal(root.toString().toByteArray())); file.finishWrite(stream) }
        catch (e: Exception) { file.failWrite(stream); throw e }
    }
    fun state(public: Boolean = false): JSONObject = synchronized(lock) {
        val state = read().optJSONObject("state") ?: defaults()
        if (public) {
            val sources = state.getJSONArray("sources")
            for (i in 0 until sources.length()) secretKeys.forEach { sources.getJSONObject(i).remove(it) }
            state.optJSONObject("current")?.put("url", "")
        }
        state
    }
    fun save(state: JSONObject): JSONObject = synchronized(lock) {
        require(state.optInt("version") == 1)
        val root = read()
        val old = root.optJSONObject("state")?.optJSONArray("sources") ?: JSONArray()
        val incoming = JSONObject(state.toString())
        val sources = incoming.getJSONArray("sources")
        require(sources.length() <= 100) { "Maximum 100 sources" }
        val ids = mutableSetOf<String>()
        for (i in 0 until sources.length()) {
            val source = sources.getJSONObject(i)
            require(source.getString("id").length in 1..128 && ids.add(source.getString("id"))) { "Invalid source ID" }
            require(source.getString("kind") in listOf("local", "url", "manifest", "webdav", "drive", "photos"))
            require(source.getString("location").length <= 8192)
            SourceFolders.normalize(source)
            SourceFolders.relayScope(source, root.optJSONObject("relayPending:${source.getString("id")}")?.takeIf { it.optString("phase") == "ready" } ?: root.optJSONObject("relay:${source.getString("id")}"))
            val previous = (0 until old.length()).map { old.getJSONObject(it) }.find { it.optString("id") == source.getString("id") }
            secretKeys.forEach { k -> if (!source.has(k) && previous?.optString("kind") == source.optString("kind") && previous.has(k)) source.put(k, previous.get(k)) }
        }
        // Invalidate before publishing the new configuration. A cached recursive listing
        // or cached authenticated image must not survive a provider credential edit.
        for (i in 0 until old.length()) {
            val previous = old.getJSONObject(i); val id = previous.getString("id")
            val next = (0 until sources.length()).map { sources.getJSONObject(it) }.find { it.optString("id") == id }
            if (next == null || MediaSafety.sourceConfig(previous) != MediaSafety.sourceConfig(next)) invalidateOwnedCache(root, id)
        }
        val settings = incoming.getJSONObject("settings")
        settings.put("intervalSeconds", settings.optLong("intervalSeconds", 900).coerceIn(900, 604800))
        settings.put("slideshowSeconds", settings.optInt("slideshowSeconds", 30).coerceIn(5, 86400))
        settings.put("cacheLimitMb", settings.optInt("cacheLimitMb", 256).coerceIn(16, 2048))
        // UI state cannot forge native wallpaper success.
        incoming.put("current", root.optJSONObject("state")?.opt("current") ?: JSONObject.NULL)
        incoming.put("lastChanged", root.optJSONObject("state")?.opt("lastChanged") ?: JSONObject.NULL)
        root.put("state", incoming).put("revision", root.optLong("revision") + 1)
        for (i in 0 until sources.length()) {
            val source = sources.getJSONObject(i); val id = source.getString("id")
            val type = if (source.optString("kind") == "photos") "google" else if (source.optString("kind") == "drive") "relay" else if (source.optString("kind") == "local" && source.optString("location").startsWith("wallta-import://")) "import" else continue
            val pending = root.optJSONObject("${type}Pending:$id") ?: continue
            val location = if (type == "google") pending.optString("deviceId") else if (type == "import") pending.optString("location") else pending.optString("folder")
            val connectedWithoutFolder = type == "relay" && !source.optBoolean("enabled", true) && source.optString("location").isBlank()
            if (pending.optString("phase") == "ready" && (source.optString("location") == location || connectedWithoutFolder)) {
                if (type == "relay") {
                    val active = root.optJSONObject("relay:$id")
                    if (active?.optString("nonce") != pending.optString("nonce") || active?.optString("relayUrl") != pending.optString("relayUrl")) {
                        invalidateOwnedCache(root, id)
                    }
                }
                pending.put("pending", false); root.put("$type:$id", pending); root.remove("${type}Pending:$id")
            }
        }
        write(root)
        this.state(true)
    }
    /** Runs only under lock against the same root that save() will commit. Grants stay intact. */
    private fun invalidateOwnedCache(root: JSONObject, sourceId: String) {
        val ledger = root.optJSONObject("cacheOwners") ?: JSONObject()
        ledger.keys().asSequence().toList().filter { ledger.getJSONObject(it).optString("source") == sourceId && it.matches(Regex("[a-f0-9]{64}")) }.forEach { mediaId ->
            File(File(context.cacheDir, "media"), "$mediaId.img").delete()
            File(File(context.cacheDir, "visual"), "$mediaId.jpg").delete()
            ledger.remove(mediaId)
        }
        root.put("cacheOwners", ledger); root.remove("catalog:$sourceId")
        root.put("cacheEpoch", JSONObject().put("value", (root.optJSONObject("cacheEpoch")?.optLong("value") ?: 0L) + 1))
    }
    fun revision(): Long = synchronized(lock) { read().optLong("revision") }
    /** Fresh-root deletion cannot be undone by save() preserving the old current image. */
    fun removeUpscaleCollection(location: String) = synchronized(lock) {
        val root = read(); val state = root.optJSONObject("state") ?: defaults()
        val sources = state.getJSONArray("sources"); val kept = JSONArray(); val removed = mutableSetOf<String>()
        for (i in 0 until sources.length()) {
            val source = sources.getJSONObject(i)
            if (source.optString("kind") == "local" && source.optString("location") == location) removed += source.getString("id") else kept.put(source)
        }
        removed.forEach { id -> invalidateOwnedCache(root, id); root.remove("import:$id"); root.remove("importPending:$id") }
        if (state.optJSONObject("current")?.optString("sourceId") in removed) state.put("current", JSONObject.NULL).put("running", false)
        state.put("sources", kept)
        root.put("state", state).put("revision", root.optLong("revision") + 1)
        write(root)
    }
    fun invalidateCache() = synchronized(lock) {
        val root = read(); root.put("revision", root.optLong("revision") + 1)
        root.put("cacheEpoch", JSONObject().put("value", (root.optJSONObject("cacheEpoch")?.optLong("value") ?: 0L) + 1)); write(root)
    }
    fun mutateState(block: (JSONObject) -> Unit): JSONObject = synchronized(lock) {
        val root = read(); val s = root.optJSONObject("state") ?: defaults(); block(s); root.put("state", s); write(root); state(true)
    }
    fun get(key: String): JSONObject? = synchronized(lock) { read().optJSONObject(key) }
    fun put(key: String, value: JSONObject?) = synchronized(lock) { val r = read(); if (value == null) r.remove(key) else r.put(key, value); write(r) }
    fun beginPending(type: String, id: String, value: JSONObject): JSONObject = synchronized(lock) {
        val root = read(); val now = System.currentTimeMillis()
        val pending = root.keys().asSequence().filter { it.startsWith("googlePending:") || it.startsWith("relayPending:") || it.startsWith("importPending:") }.toList()
        val removedImports = mutableListOf<String>()
        pending.filter { now - (root.optJSONObject(it)?.optLong("createdAt") ?: 0) > 60 * 60 * 1000 }.forEach { if (it.startsWith("importPending:")) root.getJSONObject(it).optString("location").takeIf { location -> location.isNotEmpty() }?.let { location -> removedImports += location }; root.remove(it) }
        val remaining = root.keys().asSequence().filter { it.startsWith("googlePending:") || it.startsWith("relayPending:") || it.startsWith("importPending:") }.sortedBy { root.getJSONObject(it).optLong("createdAt") }.toList()
        remaining.take((remaining.size - 31).coerceAtLeast(0)).forEach { if (it.startsWith("importPending:")) root.getJSONObject(it).optString("location").takeIf { location -> location.isNotEmpty() }?.let { location -> removedImports += location }; root.remove(it) }
        value.put("pending", true).put("nonce", java.util.UUID.randomUUID().toString()).put("createdAt", now)
        root.put("${type}Pending:$id", value); write(root)
        removedImports.forEach { location -> Net.cleanup { PhotoImporter.directory(context, location).deleteRecursively() } }
        value
    }
    fun session(type: String, id: String, activeOnly: Boolean = false): JSONObject? = synchronized(lock) {
        val root = read()
        (if (activeOnly) null else root.optJSONObject("${type}Pending:$id")) ?: root.optJSONObject("$type:$id")
    }
    fun saveSession(type: String, id: String, value: JSONObject) = synchronized(lock) {
        val root = read(); val key = if (value.optBoolean("pending")) "${type}Pending:$id" else "$type:$id"
        check(root.optJSONObject(key)?.optString("nonce") == value.optString("nonce")) { "Google operation cancelled by a newer request" }
        root.put(key, value); write(root)
    }
    fun removeSession(type: String, id: String, value: JSONObject) = synchronized(lock) {
        val root = read(); val key = if (value.optBoolean("pending")) "${type}Pending:$id" else "$type:$id"
        if (root.optJSONObject(key)?.optString("nonce") == value.optString("nonce")) { root.remove(key); write(root) }
    }
    fun takePending(type: String, id: String): JSONObject? = synchronized(lock) {
        val root = read(); val key = "${type}Pending:$id"; val value = root.optJSONObject(key); root.remove(key); write(root); value
    }
    fun setRunning(running: Boolean): JSONObject = synchronized(lock) { val s = state(); s.put("running", running); save(s) }
    private fun defaults() = JSONObject("""{"version":1,"sources":[],"settings":{"intervalSeconds":900,"slideshowSeconds":30,"order":"shuffle","fit":"fill","blurBackground":false,"autostart":false,"wifiOnly":false,"cacheLimitMb":256,"scheduleEnabled":false,"days":[0,1,2,3,4,5,6],"startTime":"07:00","endTime":"23:00","showClock":false},"running":false,"current":null,"lastChanged":null,"error":null}""")
}
