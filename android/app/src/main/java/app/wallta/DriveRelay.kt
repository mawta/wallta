package app.wallta

import org.json.JSONObject

class DriveRelay(private val store: StateStore) {
    companion object {
        fun folder(location: String): String {
            val id = Regex("/folders/([A-Za-z0-9_-]+)").find(location)?.groupValues?.get(1) ?: location
            require(id.matches(Regex("[A-Za-z0-9_-]+"))) { "Enter a Drive folder ID or folder link before connecting" }
            return id
        }
        fun base(url: String): String {
            require(Policy.http(url) && url.startsWith("https://")) { "The Drive relay must use HTTPS without embedded credentials" }
            val uri = java.net.URI(url)
            require(uri.rawQuery == null && uri.rawFragment == null) { "Enter the relay base URL without a query or fragment" }
            return url.trimEnd('/')
        }
    }
    fun session(id: String) = store.get("relay:$id")
    fun begin(args: JSONObject): JSONObject {
        val url = args.optString("relayUrl")
        if (url.isBlank()) return JSONObject().put("phase", "error").put("message", "Private Drive sign-in requires an HTTPS Wallta OAuth relay. Enter its URL, or use an API key for a public folder or a manual access token.")
        val id = args.getString("sourceId"); val relay = base(url); val folder = folder(args.getString("location"))
        val data = Net.json("$relay/api/pair", method = "POST", body = JSONObject().put("folderId", folder).put("displayName", args.optString("displayName", "Wallta TV")))
        val verification = data.getString("verificationUrl"); require(verification.startsWith("https://") && Policy.http(verification)) { "Relay returned an unsafe verification URL" }
        val j = JSONObject().put("phase", "authorizing").put("relayUrl", relay).put("folder", folder).put("deviceCode", data.getString("deviceCode")).put("verificationUrl", verification)
            .put("interval", data.optInt("pollIntervalSeconds", 5).coerceAtLeast(5)).put("nextPoll", System.currentTimeMillis() + data.optInt("pollIntervalSeconds", 5).coerceAtLeast(5) * 1000)
        disconnect(id); store.put("relay:$id", j); return public(j)
    }
    private fun public(j: JSONObject) = JSONObject().put("phase", j.optString("phase")).put("verificationUrl", j.optString("verificationUrl")).put("pollIntervalSeconds", j.optInt("interval", 5)).put("message", j.optString("message")).apply {
        if (j.optString("phase") == "ready") { put("location", j.getString("folder")); put("sourceKind", "drive") }
    }
    fun poll(id: String): JSONObject {
        val j = session(id) ?: error("Start Drive sign-in again")
        if (j.optString("phase") != "authorizing" || System.currentTimeMillis() < j.optLong("nextPoll")) return public(j)
        j.put("nextPoll", System.currentTimeMillis() + j.optInt("interval", 5) * 1000); store.put("relay:$id", j)
        val data = Net.json("${j.getString("relayUrl")}/api/pair/poll", method = "POST", body = JSONObject().put("deviceCode", j.getString("deviceCode")))
        when (data.optString("phase")) {
            "ready" -> { require(data.optString("token").isNotBlank()) { "Relay returned no connection capability" }; j.put("phase", "ready").put("token", data.getString("token")).put("folder", folder(data.optString("location", j.getString("folder")))); j.remove("deviceCode") }
            "error" -> j.put("phase", "error").put("message", data.optString("message", "Drive sign-in expired. Start again."))
            "authorizing" -> Unit
            else -> error("Unexpected relay response")
        }
        store.put("relay:$id", j); return public(j)
    }
    fun disconnect(id: String) {
        val j = session(id); store.put("relay:$id", null)
        if (j?.optString("token")?.isNotBlank() == true) runCatching { Net.bytes("${j.getString("relayUrl")}/api/connection", 1024 * 1024, mapOf("Authorization" to "Bearer ${j.getString("token")}"), "DELETE") }
    }
}
