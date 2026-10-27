package app.wallta

import android.app.UiModeManager
import android.content.Context
import android.content.res.Configuration
import org.json.JSONArray
import org.json.JSONObject
import java.time.LocalDate
import java.util.UUID

/** Device authorization is supported for Photos Ambient, never for Drive readonly. */
class GoogleAmbient(private val context: Context, private val store: StateStore = StateStore(context)) {
    private val api = "https://photosambient.googleapis.com/v1"
    private fun session(id: String) = store.get("google:$id") ?: error("Connect Google Photos again")
    private fun save(id: String, j: JSONObject) { store.put("google:$id", j) }
    private fun interval(value: String) = (value.removeSuffix("s").toDoubleOrNull() ?: 5.0).coerceAtLeast(5.0)
    fun begin(args: JSONObject): JSONObject {
        if (args.optString("kind") == "drive") return DriveRelay(store).begin(args)
        require(args.optString("kind") == "photos") { "Unsupported Google source" }
        if (context.getSystemService(UiModeManager::class.java).currentModeType != Configuration.UI_MODE_TYPE_TELEVISION) return JSONObject().put("phase", "error").put("message", "Photos Ambient is for supported TV devices. On this phone, choose Thư mục thiết bị to select a local/cloud DocumentsProvider folder, or use an online manifest. Google Photos does not expose a browsable folder through Android SAF on every device.")
        val id = args.getString("sourceId"); require(id.length in 1..128)
        val client = args.getString("clientId").trim(); require(client.endsWith(".apps.googleusercontent.com")) { "Enter a TV and Limited Input OAuth client ID" }
        val request = UUID.randomUUID().toString(); val name = args.optString("displayName", "Wallta TV").take(100).ifBlank { "Wallta TV" }
        val data = Net.json("https://oauth2.googleapis.com/device/code", method = "POST", body = JSONObject().put("client_id", client).put("scope", "profile https://www.googleapis.com/auth/photosambient.mediaitems").put("state", JSONObject().put("requestId", request).put("displayName", name).toString()))
        val j = JSONObject().put("phase", "authorizing").put("clientId", client).put("clientSecret", args.optString("clientSecret")).put("requestId", request).put("displayName", name)
            .put("deviceCode", data.getString("device_code")).put("userCode", data.getString("user_code")).put("verificationUrl", data.optString("verification_url", data.optString("verification_uri")))
            .put("interval", data.optInt("interval", 5).coerceAtLeast(5)).put("expires", System.currentTimeMillis() + data.optLong("expires_in", 1800) * 1000).put("nextPoll", System.currentTimeMillis() + data.optInt("interval", 5).coerceAtLeast(5) * 1000)
        disconnect(id); save(id, j); return public(j)
    }
    private fun public(j: JSONObject) = JSONObject().put("phase", j.optString("phase", "error")).put("verificationUrl", j.optString("verificationUrl")).put("userCode", j.optString("userCode"))
        .put("pollIntervalSeconds", j.optDouble("interval", 5.0)).put("message", j.optString("message")).apply { if (j.optString("deviceId").isNotBlank()) { put("location", j.getString("deviceId")); put("sourceKind", "photos") } }
    fun poll(id: String): JSONObject {
        if (DriveRelay(store).session(id) != null) return DriveRelay(store).poll(id)
        var j = session(id); val now = System.currentTimeMillis()
        if (now < j.optLong("nextPoll")) return public(j)
        if (j.optString("phase") == "authorizing") {
            if (now >= j.optLong("expires")) { j.put("phase", "error").put("message", "Sign-in expired. Start again."); save(id, j); return public(j) }
            j.put("nextPoll", now + j.optLong("interval", 5) * 1000); save(id, j)
            try {
                val data = Net.form("https://oauth2.googleapis.com/token", mapOf("client_id" to j.getString("clientId"), "client_secret" to j.optString("clientSecret"), "device_code" to j.getString("deviceCode"), "grant_type" to "urn:ietf:params:oauth:grant-type:device_code"))
                j.put("accessToken", data.getString("access_token")).put("refreshToken", data.optString("refresh_token")).put("tokenExpires", now + data.optLong("expires_in", 3600) * 1000).put("phase", "creating")
                j.remove("deviceCode"); save(id, j)
            } catch (e: ApiError) {
                when (e.payload.optString("error")) {
                    "authorization_pending" -> return public(j)
                    "slow_down" -> { j.put("interval", j.optLong("interval", 5) + 5).put("nextPoll", now + (j.optLong("interval") * 1000)); save(id, j); return public(j) }
                    else -> { j.put("phase", "error").put("message", "Google sign-in was denied or expired. Start again."); save(id, j); return public(j) }
                }
            }
        }
        if (j.optString("phase") == "creating") {
            val device = Net.json("$api/devices?requestId=${Net.enc(j.getString("requestId"))}", mapOf("Authorization" to "Bearer ${token(id)}"), "POST", JSONObject().put("displayName", j.getString("displayName")))
            j = session(id); updateDevice(j, device); save(id, j)
        } else if (j.optString("phase") in listOf("selecting", "ready")) {
            val device = try { Net.json("$api/devices/${Net.enc(j.getString("deviceId"))}", mapOf("Authorization" to "Bearer ${token(id)}")) }
            catch (e: ApiError) { if (e.status in listOf(401, 403, 404)) { disconnect(id); return JSONObject().put("phase", "error").put("message", "Photos access was revoked. Connect again.") }; throw e }
            j = session(id); updateDevice(j, device); save(id, j)
            if (!device.optBoolean("mediaSourcesSet")) MediaRepository(context, store).clearSource(id)
        }
        return public(j)
    }
    private fun updateDevice(j: JSONObject, device: JSONObject) {
        val seconds = interval(device.optJSONObject("pollingConfig")?.optString("pollInterval") ?: "5s")
        j.put("deviceId", device.getString("id")).put("verificationUrl", device.optString("settingsUri")).put("phase", if (device.optBoolean("mediaSourcesSet")) "ready" else "selecting")
            .put("userCode", "").put("interval", seconds).put("nextPoll", System.currentTimeMillis() + (seconds * 1000).toLong())
    }
    fun token(id: String): String {
        val j = session(id)
        if (System.currentTimeMillis() + 60000 < j.optLong("tokenExpires") && j.optString("accessToken").isNotEmpty()) return j.getString("accessToken")
        require(j.optString("refreshToken").isNotBlank()) { "Google sign-in expired. Connect again." }
        val data = try { Net.form("https://oauth2.googleapis.com/token", mapOf("client_id" to j.getString("clientId"), "client_secret" to j.optString("clientSecret"), "refresh_token" to j.getString("refreshToken"), "grant_type" to "refresh_token")) }
        catch (e: ApiError) { if (e.status in listOf(400, 401, 403)) { MediaRepository(context, store).clearSource(id); store.put("google:$id", null) }; throw e }
        j.put("accessToken", data.getString("access_token")).put("tokenExpires", System.currentTimeMillis() + data.optLong("expires_in", 3600) * 1000)
        if (data.has("refresh_token")) j.put("refreshToken", data.getString("refresh_token"))
        save(id, j); return j.getString("accessToken")
    }
    fun media(source: JSONObject): List<Media> {
        val id = source.getString("id"); val j = session(id)
        require(j.optString("phase") == "ready" && j.optString("deviceId") == source.getString("location")) { "Select Photos albums on your phone first" }
        // Poll the device at catalog refresh, so deselection/revocation clears cached media.
        val status = poll(id); require(status.optString("phase") == "ready") { "Photos selection is not ready" }
        var page = ""; val items = mutableListOf<Media>(); var pages = 0
        do {
            val s = session(id); val today = LocalDate.now().toString()
            if (s.optString("quotaDate") != today) s.put("quotaDate", today).put("quotaCount", 0)
            require(s.optInt("quotaCount") < 240) { "Photos daily request limit reached" }
            s.put("quotaCount", s.optInt("quotaCount") + 1); save(id, s)
            val data = Net.json("$api/mediaItems?deviceId=${Net.enc(source.getString("location"))}&pageSize=100&pageToken=${Net.enc(page)}", mapOf("Authorization" to "Bearer ${token(id)}"))
            val a = data.optJSONArray("mediaItems") ?: JSONArray()
            for (i in 0 until a.length()) {
                val item = a.getJSONObject(i); val file = item.optJSONObject("mediaFile") ?: continue
                if (file.optString("mimeType").startsWith("image/")) { val url = file.getString("baseUrl"); require(url.startsWith("https://")); items += Media(Policy.hash("$id:${item.getString("id")}"), file.optString("filename", "Google Photos"), "$url=w2560-h1440", id, true) }
            }
            page = data.optString("nextPageToken")
        } while (page.isNotBlank() && ++pages < 3)
        return items.distinctBy { it.id }
    }
    fun disconnect(id: String) {
        DriveRelay(store).disconnect(id)
        val j = store.get("google:$id")
        // Forget locally even if the device is offline or Google has already revoked access.
        MediaRepository(context, store).clearSource(id); store.put("google:$id", null)
        if (j != null) {
            if (j.optString("deviceId").isNotBlank() && j.optString("accessToken").isNotBlank()) runCatching { Net.bytes("$api/devices/${Net.enc(j.getString("deviceId"))}", 1024 * 1024, mapOf("Authorization" to "Bearer ${j.getString("accessToken")}"), "DELETE") }
            j.optString("refreshToken", j.optString("accessToken")).takeIf { it.isNotBlank() }?.let { runCatching { Net.form("https://oauth2.googleapis.com/revoke", mapOf("token" to it)) } }
        }
    }
}
