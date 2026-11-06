package app.wallta

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject

/** Apps Script ContentService redirects a completed POST to a one-use GET response URL. */
object GoogleBrokerTransport {
    fun request(url: String, body: JSONObject, tag: String?): JSONObject {
        require(url == GoogleConnections.BROKER_URL) { "Unrecognized Google connection service" }
        val request = Request.Builder().url(url).tag(setOfNotNull(tag)).post(body.toString().toRequestBody("application/json".toMediaType())).build()
        val bytes = Net.client.newCall(request).execute().use { response ->
            if (response.code in listOf(302, 303)) {
                val next = response.request.url.resolve(response.header("Location") ?: error("Missing Google response redirect"))?.toString() ?: error("Invalid Google response redirect")
                require(GoogleConnectionPolicy.contentRedirect(url, next, response.code)) { "Unsafe Google response redirect" }
                // No POST body, bearer token or capability is forwarded to the redirected host.
                Net.bytes(next, 65536, tag = tag, allowedUrl = { target -> GoogleConnectionPolicy.contentRedirect(url, target, 302) })
            } else {
                check(response.isSuccessful) { "Google connection service HTTP ${response.code}" }
                Net.bounded(response.body?.byteStream() ?: error("Empty Google response"), 65536)
            }
        }
        return GoogleConnectionPolicy.brokerData(JSONObject(String(bytes, Charsets.UTF_8)))
    }
}
