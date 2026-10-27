package app.wallta

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.webkit.WebView
import android.widget.TextView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature

class MainActivity : Activity() {
    private lateinit var web: WebView
    private var bridge: WalltaBridge? = null
    private var folderReply: ((String?, String?) -> Unit)? = null
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            setContentView(TextView(this).apply { text = "Wallta needs an updated Android System WebView. Update it through Google Play or your TV firmware, then reopen Wallta."; textSize = 24f; setPadding(48, 48, 48, 48) }); return
        }
        web = TrustedWebView.create(this); bridge = WalltaBridge(this, web)
        WebViewCompat.addWebMessageListener(web, "WalltaAndroid", setOf(TrustedWebView.origin)) { _, message, sourceOrigin, isMainFrame, _ ->
            if (isMainFrame && TrustedWebView.trusted(sourceOrigin)) message.data?.let { bridge?.receive(it) }
        }
        web.setOnApplyWindowInsetsListener { view, insets ->
            @Suppress("DEPRECATION")
            view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets
        }
        setContentView(web); web.loadUrl("${TrustedWebView.origin}/index.html")
        Thread { runCatching { WallpaperWorker.schedule(applicationContext) } }.start()
    }
    fun pickFolder(reply: (String?, String?) -> Unit) {
        if (folderReply != null) { reply(null, "A folder picker is already open"); return }
        folderReply = reply
        try { startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION), 41) }
        catch (e: Exception) { folderReply = null; reply(null, "This firmware has no folder picker. Install a DocumentsProvider file manager, or use an online source.") }
    }
    @Deprecated("Android activity result API")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 41) return
        val reply = folderReply; folderReply = null
        val uri = data?.data
        if (resultCode != RESULT_OK || uri == null) { reply?.invoke(null, "Folder selection cancelled"); return }
        try { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION); reply?.invoke(uri.toString(), null) }
        catch (e: Exception) { reply?.invoke(null, "Could not retain folder permission. Choose another folder.") }
    }
    override fun onDestroy() {
        folderReply?.invoke(null, "Activity closed"); folderReply = null; bridge?.close()
        if (::web.isInitialized) { web.stopLoading(); WebViewCompat.removeWebMessageListener(web, "WalltaAndroid"); web.destroy() }
        super.onDestroy()
    }
}
