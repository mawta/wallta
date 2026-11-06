package app.wallta

import android.app.UiModeManager
import android.content.Context
import android.content.res.Configuration
import org.json.JSONArray
import org.json.JSONObject

/** Current Google connections. Legacy relay/Ambient sessions retain their own dispatcher. */
class GoogleConnections(
    private val context: Context,
    private val store: StateStore = StateStore(context),
    private val brokerRequest: (String, JSONObject, String?) -> JSONObject = GoogleBrokerTransport::request,
    private val authorization: (String, String, String?, Boolean) -> GoogleGrant = { id, kind, account, interactive -> GoogleAuthorization(context).authorize(id, kind, account, interactive) },
    private val cleanup: (() -> Unit) -> Unit = Net::cleanup,
    private val googleRequest: (String, Map<String, String>, String, JSONObject?, String?) -> JSONObject = Net::json,
    private val importTask: (() -> Unit) -> Unit = { task -> importExecutor.execute(task) }
) {
    companion object {
        const val TYPE = "googleConnection"
        const val BROKER_URL = "https://script.google.com/macros/s/AKfycbyyn3OoEYI4MP0Yu3AbSvVRkUVSsT9Onl6_H3QHLlDyI0JA3aE5Flw_o2XQiH4xtxeY/exec"
        private const val PICKER = "https://photospicker.googleapis.com/v1"
        private val importExecutor = java.util.concurrent.Executors.newFixedThreadPool(2)
        private val importing = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
    }
    fun session(id: String, activeOnly: Boolean = false) = store.session(TYPE, id, activeOnly)
    private fun tv() = context.getSystemService(UiModeManager::class.java).currentModeType == Configuration.UI_MODE_TYPE_TELEVISION
    fun options(): JSONObject {
        val native = !tv() && GoogleAuthorization.available(context)
        val configured = if (native) true else runCatching {
            val data = JSONObject(String(Net.bytes(BROKER_URL, 65536, allowedUrl = { url -> url == BROKER_URL || GoogleConnectionPolicy.contentRedirect(BROKER_URL, url, 302) }), Charsets.UTF_8))
            data.optString("service") == "wallta-google-auth" && data.optInt("protocol") == 1 && data.optBoolean("configured")
        }.getOrDefault(false)
        return JSONObject().put("mode", if (native) "native" else "broker").put("brokerUrl", if (native) JSONObject.NULL else BROKER_URL).put("configured", configured)
    }
    fun begin(args: JSONObject): JSONObject {
        val id = args.getString("sourceId"); require(id.length in 1..128)
        val kind = args.getString("kind"); GoogleAuthorization.scope(kind)
        if (args.optString("method") == "files" && kind == "photos") return PhotoImporter(context, store).begin(id)
        cancel(id)
        GoogleAmbient(context, store).cancel(id)
        val mode = if (args.optString("mode") == "broker" || tv() || !GoogleAuthorization.available(context)) "broker" else "native"
        val j = store.beginPending(TYPE, id, JSONObject().put("kind", kind).put("mode", mode).put("phase", "authorizing").put("interval", 5)
            .put("expires", System.currentTimeMillis() + 600000).put("brokerUrl", BROKER_URL))
        try {
            if (mode == "native") {
                val grant = authorization(id, kind, null, true)
                require(grant.account.contains('@')) { "Google did not return the selected account" }
                j.put("account", grant.account).put("accessToken", grant.accessToken).put("tokenExpires", System.currentTimeMillis() + 300000)
                    .put("authorized", true).put("phase", if (kind == "photos") "authorizing" else "ready")
            } else {
                val data = brokerRequest(BROKER_URL, JSONObject().put("action", "pair").put("kind", kind).put("displayName", args.optString("displayName", "Wallta").take(80)), j.getString("nonce"))
                j.put("deviceCode", data.getString("deviceCode"))
                require(j.getString("deviceCode").matches(Regex("[a-f0-9]{64}"))) { "Google service returned an invalid pairing code" }
                val uri = java.net.URI(data.getString("verificationUrl"))
                require(uri.scheme == "https" && uri.host == "script.google.com" && uri.userInfo == null && uri.fragment == null && uri.path == java.net.URI(BROKER_URL).path) { "Google service returned an unsafe sign-in link" }
                j.put("verificationUrl", uri.toString()).put("userCode", data.optString("displayCode"))
                    .put("expires", System.currentTimeMillis() + data.optLong("expiresIn", 600).coerceIn(1, 600) * 1000)
                    .put("nextPoll", System.currentTimeMillis() + 5000)
            }
            persist(id, j)
            if (mode == "native" && kind == "photos") return createPicker(id, j)
            return GoogleConnectionPolicy.public(j)
        } catch (e: Exception) {
            store.removeSession(TYPE, id, j); cleanup { cleanupRemote(j) }; throw e
        }
    }
    fun poll(id: String): JSONObject {
        val j = session(id) ?: return GoogleAmbient(context, store).poll(id)
        if (j.optString("phase") in listOf("ready", "error")) return GoogleConnectionPolicy.public(j)
        if (j.optBoolean("importing")) return startImport(id, j)
        val now = System.currentTimeMillis()
        if (now >= j.optLong("expires")) {
            j.put("phase", "error").put("message", "Google selection expired. Connect again."); persist(id, j); cleanup { cleanupRemote(j) }
            return GoogleConnectionPolicy.public(j)
        }
        if (now < j.optLong("nextPoll")) return GoogleConnectionPolicy.public(j)
        j.put("nextPoll", now + (j.optDouble("interval", 5.0) * 1000).toLong()); persist(id, j)
        if (j.optBoolean("authorized") && j.optString("kind") == "photos" && j.optString("phase") == "authorizing") return createPicker(id, j)
        if (j.optString("phase") == "authorizing") {
            val data = brokerRequest(j.getString("brokerUrl"), JSONObject().put("action", "poll").put("deviceCode", j.getString("deviceCode")), j.getString("nonce"))
            when (data.optString("phase")) {
                "ready" -> {
                    j.put("connectionToken", data.getString("connectionToken"))
                    require(j.getString("connectionToken").matches(Regex("[a-f0-9]{64}"))) { "Google service returned an invalid capability" }
                    require(data.optString("kind") == j.getString("kind") && data.optString("account").contains('@')) { "Google service returned a different connection" }
                    j.put("account", data.getString("account")).put("authorized", true).put("phase", if (j.getString("kind") == "photos") "authorizing" else "ready").put("verificationUrl", "").put("userCode", "")
                    persist(id, j)
                    if (j.getString("kind") == "photos") return createPicker(id, j)
                }
                "error" -> { j.put("phase", "error").put("message", data.optString("message", "Google sign-in failed. Connect again.")); persist(id, j) }
                "authorizing" -> Unit
                else -> error("Google service returned an invalid pairing state")
            }
        } else if (j.optString("phase") == "selecting") {
            val data = api(id, "$PICKER/sessions/${Net.enc(j.getString("pickerId"))}", expected = j)
            if (data.optBoolean("mediaItemsSet")) return startImport(id, j)
            val timeout = seconds(data.optJSONObject("pollingConfig")?.optString("timeoutIn"), 600.0)
            if (timeout <= 0) { j.put("phase", "error").put("message", "Photos selection expired. Select photos again.") }
            else {
                val interval = seconds(data.optJSONObject("pollingConfig")?.optString("pollInterval"), 5.0).coerceAtLeast(5.0)
                j.put("interval", interval).put("nextPoll", now + (interval * 1000).toLong())
                j.put("expires", minOf(j.getLong("expires"), now + (timeout * 1000).toLong()))
            }
            persist(id, j)
        }
        return GoogleConnectionPolicy.public(j)
    }
    private fun seconds(value: String?, fallback: Double): Double = value?.removeSuffix("s")?.toDoubleOrNull()?.takeIf { it.isFinite() && it >= 0 } ?: fallback
    internal fun startImport(id: String, j: JSONObject): JSONObject {
        val nonce = j.getString("nonce")
        val work = synchronized(StateStore.lock) {
            val fresh = session(id)?.takeIf { it.optString("nonce") == nonce } ?: error("Google operation cancelled")
            if (fresh.optString("phase") in listOf("ready", "error") || !importing.add(nonce)) return GoogleConnectionPolicy.public(fresh)
            try {
                fresh.put("importing", true).put("verificationUrl", "").put("message", "Importing selected photos…")
                persist(id, fresh); fresh
            } catch (e: Exception) { importing.remove(nonce); throw e }
        }
        try {
            importTask {
                try { importPicked(id, work) }
                catch (e: Exception) {
                    work.put("phase", "error").put("message", e.message ?: "Photo import failed. Try again.")
                    runCatching { persist(id, work) }
                } finally { importing.remove(nonce) }
            }
        } catch (e: Exception) { importing.remove(nonce); throw e }
        // The worker owns its snapshot. Read a separate one so polls never mutate it.
        return GoogleConnectionPolicy.public(session(id)?.takeIf { it.optString("nonce") == nonce } ?: error("Google operation cancelled"))
    }
    private fun createPicker(id: String, j: JSONObject): JSONObject {
        val data = api(id, "$PICKER/sessions", "POST", JSONObject().put("pickingConfig", JSONObject().put("maxItemCount", "50")), j)
        j.put("pickerId", data.getString("id"))
        val uri = java.net.URI(data.getString("pickerUri"))
        require(uri.scheme == "https" && uri.host == "photos.google.com" && uri.userInfo == null && uri.fragment == null) { "Google returned an unsafe Photos Picker link" }
        val interval = seconds(data.optJSONObject("pollingConfig")?.optString("pollInterval"), 5.0).coerceAtLeast(5.0)
        j.put("phase", "selecting").put("verificationUrl", uri.toString()).put("interval", interval).put("nextPoll", System.currentTimeMillis() + (interval * 1000).toLong())
        j.put("expires", minOf(System.currentTimeMillis() + 600000, runCatching { java.time.Instant.parse(data.getString("expireTime")).toEpochMilli() }.getOrDefault(Long.MAX_VALUE)))
        persist(id, j); return GoogleConnectionPolicy.public(j)
    }
    private fun importPicked(id: String, j: JSONObject): JSONObject {
        val items = mutableListOf<JSONObject>(); var page = ""; var pages = 0
        do {
            val data = api(id, "$PICKER/mediaItems?sessionId=${Net.enc(j.getString("pickerId"))}&pageSize=100&pageToken=${Net.enc(page)}", expected = j)
            val a = data.optJSONArray("mediaItems") ?: JSONArray()
            for (i in 0 until a.length()) {
                val item = a.getJSONObject(i); val file = item.optJSONObject("mediaFile") ?: continue
                if (file.optString("mimeType").startsWith("image/")) { require(items.size < 50) { "Choose at most 50 photos per import" }; items += item }
            }
            page = data.optString("nextPageToken")
        } while (page.isNotBlank() && ++pages < 10)
        require(page.isBlank()) { "Too many selected photos. Choose at most 50." }
        val result = PhotoImporter(context, store).importGoogle(id, items, j.getString("account"), j.getString("nonce")) { url ->
            require(MediaSafety.photosUrl(url)) { "Google returned an unsafe image URL" }
            Net.bytes("$url=w4096-h4096", 25 * 1024 * 1024, mapOf("Authorization" to "Bearer ${token(id, false, j.getString("nonce"))}"), tag = j.getString("nonce"), allowedUrl = MediaSafety::photosUrl)
        }
        j.put("phase", "ready").put("location", result.getString("location")).put("verificationUrl", "").put("message", result.optString("message"))
        try { persist(id, j) } catch (e: Exception) { PhotoImporter(context, store).cancel(id, result.getString("location")); throw e }
        cleanup { deletePicker(j) }; return GoogleConnectionPolicy.public(j)
    }
    fun token(id: String, activeOnly: Boolean = true, expectedNonce: String? = null): String {
        val j = session(id, activeOnly) ?: error("Reconnect Google in Wallta")
        check(expectedNonce == null || j.optString("nonce") == expectedNonce) { "Google operation cancelled by a newer request" }
        if (j.optString("accessToken").isNotBlank() && System.currentTimeMillis() + 60000 < j.optLong("tokenExpires")) return j.getString("accessToken")
        if (j.getString("mode") == "native") {
            val grant = authorization(id, j.getString("kind"), j.getString("account"), false)
            check(grant.account == j.getString("account")) { "Google account changed. Reconnect in Wallta." }
            j.put("accessToken", grant.accessToken).put("tokenExpires", System.currentTimeMillis() + 300000)
        } else {
            val data = brokerRequest(j.getString("brokerUrl"), JSONObject().put("action", "token").put("connectionToken", j.getString("connectionToken")), j.getString("nonce"))
            check(data.optString("kind") == j.getString("kind") && data.optString("account") == j.getString("account") && data.optString("accessToken").isNotBlank()) { "Google connection changed. Connect again." }
            j.put("accessToken", data.getString("accessToken")).put("tokenExpires", data.getLong("expiresAt") * 1000)
            check(j.getLong("tokenExpires") > System.currentTimeMillis()) { "Google access expired. Connect again." }
        }
        store.updateSession(TYPE, id, j.getString("nonce")) { fresh -> fresh.put("accessToken", j.getString("accessToken")).put("tokenExpires", j.getLong("tokenExpires")) }
        return j.getString("accessToken")
    }
    private fun api(id: String, url: String, method: String = "GET", body: JSONObject? = null, expected: JSONObject? = null): JSONObject {
        val j = expected ?: session(id) ?: error("Google operation cancelled")
        return try { googleRequest(url, mapOf("Authorization" to "Bearer ${token(id, false, j.getString("nonce"))}"), method, body, j.getString("nonce")) }
        catch (e: ApiError) { if (e.status in listOf(401, 403)) error("Google permission expired. Reconnect this account in Wallta."); throw e }
    }
    fun folders(id: String, parentId: String, pageToken: String, shared: Boolean): JSONObject {
        require(pageToken.length <= 8192)
        val j = session(id) ?: error("Connect Google Drive first"); require(j.getString("kind") == "drive" && j.getString("phase") == "ready")
        val q = GoogleConnectionPolicy.folderQuery(parentId, shared)
        val data = api(id, "https://www.googleapis.com/drive/v3/files?q=${Net.enc(q)}&pageSize=100&fields=nextPageToken,files(id,name)&supportsAllDrives=true&includeItemsFromAllDrives=true&orderBy=folder,name_natural&pageToken=${Net.enc(pageToken)}", expected = j)
        return JSONObject().put("folders", data.optJSONArray("files") ?: JSONArray()).put("nextPageToken", data.optString("nextPageToken")).put("account", j.getString("account"))
    }
    fun select(id: String, folderId: String): JSONObject {
        val folder = DriveRelay.folder(folderId)
        val j = session(id) ?: error("Connect Google Drive first"); require(j.getString("kind") == "drive")
        val data = api(id, "https://www.googleapis.com/drive/v3/files/${Net.enc(folder)}?fields=id,name,mimeType,trashed&supportsAllDrives=true", expected = j)
        require(data.optString("mimeType") == "application/vnd.google-apps.folder" && !data.optBoolean("trashed")) { "Choose an available Drive folder" }
        return JSONObject().put("id", data.getString("id")).put("name", data.getString("name")).put("account", j.getString("account"))
    }
    private fun persist(id: String, j: JSONObject) {
        try {
            synchronized(StateStore.lock) {
                val fresh = session(id)?.takeIf { it.optString("nonce") == j.optString("nonce") }
                    ?: session(id, true)?.takeIf { it.optString("nonce") == j.optString("nonce") }
                    ?: error("Google operation cancelled by a newer request")
                if (fresh.optLong("tokenExpires") > j.optLong("tokenExpires")) j.put("accessToken", fresh.getString("accessToken")).put("tokenExpires", fresh.getLong("tokenExpires"))
                j.put("pending", fresh.optBoolean("pending")); store.saveSession(TYPE, id, j)
            }
        } catch (e: Exception) {
            cleanup { cleanupRemote(j) }; throw e
        }
    }
    fun cancel(id: String) {
        (context as? MainActivity)?.cancelGoogle(id)
        val j = store.takePending(TYPE, id)
        PhotoImporter(context, store).cancel(id)
        if (j != null) { Net.cancel(j.optString("nonce")); cleanup { cleanupRemote(j) } }
    }
    fun disconnect(id: String) {
        cancel(id)
        val j = session(id, true); store.put("$TYPE:$id", null)
        if (j != null) cleanup { cleanupRemote(j) }
        MediaRepository(context, store).clearSource(id)
    }
    private fun deletePicker(j: JSONObject) {
        if (j.optString("pickerId").isNotBlank() && j.optString("accessToken").isNotBlank()) runCatching {
            Net.bytes("$PICKER/sessions/${Net.enc(j.getString("pickerId"))}", 65536, mapOf("Authorization" to "Bearer ${j.getString("accessToken")}"), "DELETE")
        }
    }
    private fun cleanupRemote(j: JSONObject) {
        deletePicker(j)
        if (j.optString("mode") == "broker") {
            if (j.optString("deviceCode").isNotBlank()) runCatching { brokerRequest(j.getString("brokerUrl"), JSONObject().put("action", "cancel").put("deviceCode", j.getString("deviceCode")), null) }
            if (j.optString("connectionToken").isNotBlank()) runCatching { brokerRequest(j.getString("brokerUrl"), JSONObject().put("action", "disconnect").put("connectionToken", j.getString("connectionToken")), null) }
        }
        // Native grants may be shared by other saved sources. Forget this connection
        // without revoking the user's entire app-wide Google grant.
    }
    internal fun cleanupAbandoned(j: JSONObject) { Net.cancel(j.optString("nonce")); cleanup { cleanupRemote(j) } }
}
