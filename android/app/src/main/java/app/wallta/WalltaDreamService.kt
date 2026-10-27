package app.wallta

import android.os.Handler
import android.os.Looper
import android.service.dreams.DreamService
import android.webkit.WebView
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.Future

class WalltaDreamService : DreamService() {
    private val handler = Handler(Looper.getMainLooper())
    private var web: WebView? = null
    private var io = Executors.newSingleThreadExecutor()
    private var work: Future<*>? = null
    @Volatile private var active = false
    override fun onAttachedToWindow() {
        super.onAttachedToWindow(); isInteractive = false; isFullscreen = true; isScreenBright = true
        web = TrustedWebView.create(this).also { setContentView(it) }
        web?.loadDataWithBaseURL("${TrustedWebView.origin}/dream.html", """<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><style>html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#000}#bg{position:absolute;inset:-5%;background:center/cover;filter:blur(28px) brightness(.55)}#image{position:absolute;inset:0;width:100%;height:100%;object-fit:cover}#clock{position:absolute;right:5vw;bottom:5vh;color:white;font:4vw sans-serif;text-shadow:0 2px 6px #000}#message{position:absolute;left:6%;bottom:8%;color:white;font:24px sans-serif} </style></head><body><div id="bg"></div><img id="image"><div id="clock"></div><div id="message">Wallta · Loading photos…</div><script>window.show=function(url,fit,blur,clock){document.getElementById('image').src=url;document.getElementById('image').style.objectFit=fit==='fit'?'contain':fit==='stretch'?'fill':'cover';document.getElementById('bg').style.backgroundImage=blur?'url("'+url+'")':'none';document.getElementById('clock').style.display=clock?'block':'none';document.getElementById('message').textContent=''};window.fail=function(text){document.getElementById('message').textContent=text};setInterval(function(){document.getElementById('clock').textContent=new Date().toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})},1000)</script></body></html>""", "text/html", "UTF-8", null)
    }
    override fun onDreamingStarted() { super.onDreamingStarted(); active = true; if (io.isShutdown) io = Executors.newSingleThreadExecutor(); handler.postDelayed(slide, 400) }
    private val slide = object : Runnable {
        override fun run() {
            if (!active) return
            work = io.submit {
                val store = StateStore(applicationContext); val settings = store.state().getJSONObject("settings")
                try {
                    check(WallpaperEngine.allowed(settings)) { "Wallta is outside the configured schedule" }
                    val repo = MediaRepository(applicationContext, store); val items = repo.list(); val media = Rotation.choose(store, items, settings, "dreamRotation")
                    repo.image(media)
                    if (!active || Thread.currentThread().isInterrupted) return@submit
                    Rotation.consumed(store, items, media, "dreamRotation")
                    val url = "${TrustedWebView.origin}/media/${media.id}.img"
                    handler.post { if (active) web?.evaluateJavascript("window.show&&window.show(${JSONObject.quote(url)},${JSONObject.quote(settings.optString("fit"))},${settings.optBoolean("blurBackground")},${settings.optBoolean("showClock")})", null) }
                } catch (e: Exception) { handler.post { if (active) web?.evaluateJavascript("window.fail&&window.fail(${JSONObject.quote(e.message ?: "Add an image source in Wallta")})", null) } }
                finally { if (active) handler.postDelayed(this, settings.optLong("slideshowSeconds", 30).coerceIn(5, 86400) * 1000) }
            }
        }
    }
    private fun stop() { active = false; handler.removeCallbacksAndMessages(null); work?.cancel(true); work = null; io.shutdownNow(); web?.stopLoading() }
    override fun onDreamingStopped() { stop(); super.onDreamingStopped() }
    override fun onDetachedFromWindow() { stop(); web?.destroy(); web = null; super.onDetachedFromWindow() }
}
