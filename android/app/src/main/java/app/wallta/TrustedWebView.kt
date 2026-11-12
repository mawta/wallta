package app.wallta

import android.content.Context
import android.graphics.Color
import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.io.File

object TrustedWebView {
    const val origin = "https://appassets.androidplatform.net"
    @Volatile private var upscale: UpscaleManager? = null
    fun registerUpscale(manager: UpscaleManager) { upscale = manager }
    fun unregisterUpscale(manager: UpscaleManager) { if (upscale === manager) upscale = null }
    fun create(context: Context): WebView {
        val loader = WebViewAssetLoader.Builder().addPathHandler("/upscale/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse = upscale?.preview(path)?.let { file -> runCatching { WebResourceResponse("image/jpeg", null, file.inputStream()) }.getOrNull() } ?: denied()
        }).addPathHandler("/photo-preview/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse = PhotoPreviewCache.served(context, path)?.let { file ->
                runCatching { WebResourceResponse("image/jpeg", null, file.inputStream()) }.getOrNull()
            } ?: denied()
        }).addPathHandler("/media/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse {
                if (!path.matches(Regex("[a-f0-9]{64}\\.img"))) return denied()
                // Original images can be 120 MP. Never expose those to WebView's decoder.
                val file = VisualCache.file(context, path.substringBefore('.'))
                return if (file.isFile) WebResourceResponse("image/jpeg", null, file.inputStream()) else denied()
            }
        }).addPathHandler("/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse {
                if (path.contains("..") || path.startsWith('/')) return denied()
                val name = path.ifBlank { "index.html" }
                val mime = when (name.substringAfterLast('.')) { "html" -> "text/html"; "js" -> "application/javascript"; "css" -> "text/css"; "svg" -> "image/svg+xml"; "woff2" -> "font/woff2"; "png" -> "image/png"; "ico" -> "image/x-icon"; else -> "application/octet-stream" }
                return runCatching { WebResourceResponse(mime, "UTF-8", 200, "OK", mapOf("Content-Security-Policy" to "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self'; font-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'", "X-Content-Type-Options" to "nosniff"), context.assets.open("web/$name")) }.getOrElse { denied() }
            }
        }).build()
        return WebView(context).apply {
            setBackgroundColor(Color.BLACK)
            settings.apply {
                javaScriptEnabled = true; domStorageEnabled = true
                allowFileAccess = false; allowContentAccess = false
                @Suppress("DEPRECATION") allowFileAccessFromFileURLs = false
                @Suppress("DEPRECATION") allowUniversalAccessFromFileURLs = false
                mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
                javaScriptCanOpenWindowsAutomatically = false; setSupportMultipleWindows(false)
                mediaPlaybackRequiresUserGesture = false
            }
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse = if (trusted(request.url)) {
                    if (request.url.path?.startsWith("/playback/") == true) playback(context, request.url.path!!.removePrefix("/playback/"), request.requestHeaders.entries.firstOrNull { it.key.equals("Range", true) }?.value)
                    else loader.shouldInterceptRequest(request.url) ?: denied()
                } else denied()
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = !trusted(request.url)
            }
        }
    }
    fun trusted(uri: Uri) = uri.scheme == "https" && uri.host == "appassets.androidplatform.net" && uri.port in listOf(-1, 443) && uri.userInfo == null
    internal fun playback(context: Context, path: String, range: String? = null): WebResourceResponse {
        if (!path.matches(Regex("[a-f0-9]{64}\\.(gif|mp4|webm)"))) return denied()
        return runCatching {
            val id = path.substringBefore('.'); val store = StateStore(context)
            synchronized(StateStore.lock) {
                val owner = store.get("cacheOwners")?.optJSONObject(id) ?: return denied()
                val sources = store.state().getJSONArray("sources")
                check((0 until sources.length()).any { sources.getJSONObject(it).let { source -> source.optString("id") == owner.optString("source") && source.optBoolean("enabled", true) } })
                val file = MediaRepository(context, store).cacheFile(id)
                check(UpscaleFiles.plain(file, false) && file.length() in 1..128L * 1024 * 1024)
                val kind = MotionPolicy.fileKind(file)
                check(kind == if (path.endsWith(".gif")) "gif" else "video")
                if (kind == "gif") {
                    val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }; android.graphics.BitmapFactory.decodeFile(file.path, bounds)
                    check(file.length() <= 25L * 1024 * 1024 && bounds.outWidth in 1..4096 && bounds.outHeight in 1..4096 && bounds.outWidth.toLong() * bounds.outHeight <= 8_388_608)
                }
                val length = file.length()
                val span = PlaybackRange.parse(range, length)
                val stream = file.inputStream()
                try {
                    var skipped = 0L
                    while (skipped < span.first) { val count = stream.skip(span.first - skipped); check(count > 0); skipped += count }
                    val remaining = span.last - span.first + 1
                    val bounded = object : java.io.FilterInputStream(stream) {
                        private var left = remaining
                        override fun read(): Int { if (left <= 0) return -1; val value = super.read(); if (value >= 0) left--; return value }
                        override fun read(buffer: ByteArray, offset: Int, count: Int): Int { if (left <= 0) return -1; val read = `in`.read(buffer, offset, minOf(count.toLong(), left).toInt()); if (read > 0) left -= read; return read }
                    }
                    val headers = mutableMapOf("Accept-Ranges" to "bytes", "Content-Length" to remaining.toString(), "Cache-Control" to "no-store", "X-Content-Type-Options" to "nosniff")
                    if (range != null) headers["Content-Range"] = "bytes ${span.first}-${span.last}/$length"
                    WebResourceResponse(if (kind == "gif") "image/gif" else if (path.endsWith(".webm")) "video/webm" else "video/mp4", null, if (range == null) 200 else 206, if (range == null) "OK" else "Partial Content", headers, bounded)
                } catch (e: Exception) { stream.close(); throw e }
            }
        }.getOrElse { denied() }
    }
    fun denied() = WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", emptyMap(), ByteArrayInputStream(ByteArray(0)))
}
