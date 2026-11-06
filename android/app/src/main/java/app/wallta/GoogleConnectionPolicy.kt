package app.wallta

import org.json.JSONObject
import java.net.URI

object GoogleConnectionPolicy {
    // Android's optString turns an explicit JSON null into the literal "null".
    fun pageToken(args: JSONObject): String = if (args.isNull("pageToken")) "" else args.getString("pageToken")
    fun brokerData(response: JSONObject): JSONObject {
        check(response.optBoolean("ok")) { response.optJSONObject("error")?.optString("message")?.take(300)?.takeIf { it.isNotBlank() } ?: "Google connection service is unavailable. Try again." }
        return response.optJSONObject("data") ?: error("Google connection service returned an invalid response")
    }
    fun contentRedirect(from: String, to: String, status: Int): Boolean = runCatching {
        val start = URI(from); val next = URI(to)
        status in listOf(302, 303) && start.scheme == "https" && start.host == "script.google.com" && start.userInfo == null &&
            next.scheme == "https" && next.host == "script.googleusercontent.com" && next.port in listOf(-1, 443) && next.userInfo == null && next.fragment == null
    }.getOrDefault(false)
    fun folderQuery(parent: String, shared: Boolean): String {
        val id = DriveRelay.folder(parent.ifBlank { "root" })
        return "trashed = false and mimeType = 'application/vnd.google-apps.folder' and " + if (shared && id == "root") "sharedWithMe" else "'$id' in parents"
    }
    fun public(value: JSONObject): JSONObject = JSONObject().put("phase", value.optString("phase", "error"))
        .put("account", value.optString("account")).put("verificationUrl", value.optString("verificationUrl"))
        .put("userCode", value.optString("userCode")).put("pollIntervalSeconds", value.optDouble("interval", 5.0))
        .put("message", value.optString("message")).apply {
            if (value.optString("phase") == "ready") {
                put("sourceKind", if (value.optString("kind") == "photos") "local" else "drive")
                put("location", value.optString("location"))
            }
        }
}
