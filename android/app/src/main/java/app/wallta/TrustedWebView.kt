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
    fun create(context: Context): WebView {
        val loader = WebViewAssetLoader.Builder().addPathHandler("/media/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse {
                if (!path.matches(Regex("[a-f0-9]{64}\\.img"))) return denied()
                val file = File(File(context.cacheDir, "media"), path)
                return if (file.isFile) WebResourceResponse("image/jpeg", null, file.inputStream()) else denied()
            }
        }).addPathHandler("/", object : WebViewAssetLoader.PathHandler {
            override fun handle(path: String): WebResourceResponse {
                if (path.contains("..") || path.startsWith('/')) return denied()
                val name = path.ifBlank { "index.html" }
                val mime = when (name.substringAfterLast('.')) { "html" -> "text/html"; "js" -> "application/javascript"; "css" -> "text/css"; "svg" -> "image/svg+xml"; "woff2" -> "font/woff2"; "png" -> "image/png"; "ico" -> "image/x-icon"; else -> "application/octet-stream" }
                return runCatching { WebResourceResponse(mime, "UTF-8", 200, "OK", mapOf("Content-Security-Policy" to "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'", "X-Content-Type-Options" to "nosniff"), context.assets.open("web/$name")) }.getOrElse { denied() }
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
                mediaPlaybackRequiresUserGesture = true
            }
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse = if (trusted(request.url)) loader.shouldInterceptRequest(request.url) ?: denied() else denied()
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = !trusted(request.url)
            }
        }
    }
    fun trusted(uri: Uri) = uri.scheme == "https" && uri.host == "appassets.androidplatform.net" && uri.port in listOf(-1, 443) && uri.userInfo == null
    fun denied() = WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", emptyMap(), ByteArrayInputStream(ByteArray(0)))
}
