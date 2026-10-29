package app.wallta

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.InputStream
import java.net.URLEncoder
import java.util.concurrent.TimeUnit

object Net {
    private val requestScope = ThreadLocal<String>()
    fun <T> scoped(tag: String, block: () -> T): T {
        val previous = requestScope.get(); requestScope.set(tag)
        try { return block() } finally { if (previous == null) requestScope.remove() else requestScope.set(previous) }
    }
    private val cleanupIO = java.util.concurrent.Executors.newSingleThreadExecutor()
    fun cleanup(block: () -> Unit) { cleanupIO.execute { runCatching(block) } }
    fun cancel(tag: String) { (client.dispatcher.queuedCalls() + client.dispatcher.runningCalls()).filter { (it.request().tag() as? Set<*>)?.contains(tag) == true }.forEach { it.cancel() } }
    val client = OkHttpClient.Builder().connectTimeout(15, TimeUnit.SECONDS).readTimeout(30, TimeUnit.SECONDS).callTimeout(60, TimeUnit.SECONDS).followRedirects(false).followSslRedirects(false).build()
    fun enc(s: String) = URLEncoder.encode(s, "UTF-8")
    fun bounded(input: InputStream, limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream(); val buffer = ByteArray(32768); var total = 0
        while (true) { check(!Thread.currentThread().isInterrupted) { "Read operation cancelled" }; val n = input.read(buffer); if (n < 0) break; total += n; require(total <= limit) { "Response exceeds size limit" }; out.write(buffer, 0, n) }
        return out.toByteArray()
    }
    fun bytes(url: String, limit: Int, headers: Map<String, String> = emptyMap(), method: String = "GET", body: String? = null, type: String = "application/json", tag: String? = null, allowedUrl: ((String) -> Boolean)? = null): ByteArray {
        require(Policy.http(url)) { "Only HTTP(S) sources are supported" }
        var target = url; var auth = headers
        repeat(6) {
            check(!Thread.currentThread().isInterrupted) { "Network operation cancelled" }
            require(allowedUrl == null || allowedUrl(target)) { "Redirect is outside the selected source folders" }
            val request = Request.Builder().url(target).tag(listOfNotNull(tag, requestScope.get()).toSet()).apply { auth.forEach { (k, v) -> header(k, v) }; method(method, body?.toRequestBody(type.toMediaType())) }.build()
            client.newCall(request).execute().use { response ->
                if (response.code in listOf(301, 302, 303, 307, 308)) {
                    require(method == "GET") { "Redirect refused for credential request" }
                    val next = response.request.url.resolve(response.header("Location") ?: error("Missing redirect")) ?: error("Invalid redirect")
                    require(next.isHttps || !response.request.url.isHttps) { "HTTPS downgrade refused" }
                    if (next.host != response.request.url.host || next.port != response.request.url.port) auth = emptyMap()
                    target = next.toString()
                } else {
                    val bytes = bounded(response.body?.byteStream() ?: error("Empty response"), limit)
                    if (!response.isSuccessful) throw ApiError(response.code, runCatching { JSONObject(String(bytes)) }.getOrDefault(JSONObject()))
                    return bytes
                }
            }
        }
        error("Too many redirects")
    }
    fun json(url: String, headers: Map<String, String> = emptyMap(), method: String = "GET", body: JSONObject? = null, tag: String? = null): JSONObject = JSONObject(String(bytes(url, 4 * 1024 * 1024, headers, method, body?.toString(), tag = tag), Charsets.UTF_8))
    fun form(url: String, values: Map<String, String>, tag: String? = null): JSONObject = JSONObject(String(bytes(url, 1024 * 1024, method = "POST", body = values.entries.joinToString("&") { enc(it.key) + "=" + enc(it.value) }, type = "application/x-www-form-urlencoded", tag = tag), Charsets.UTF_8))
}
class ApiError(val status: Int, val payload: JSONObject) : Exception("Provider HTTP $status")
