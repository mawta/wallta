package app.wallta

import android.app.UiModeManager
import android.app.WallpaperManager
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.provider.Settings
import android.webkit.WebView
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

class WalltaBridge(private val activity: MainActivity, private val web: WebView) {
    private val io = Executors.newFixedThreadPool(2)
    private val control = Executors.newSingleThreadExecutor()
    private val store = StateStore(activity.applicationContext)
    private val authorizationIds = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
    @Volatile private var closed = false
    fun receive(message: String) {
        if (closed || message.length > 1024 * 1024) return
        val request = runCatching { JSONObject(message) }.getOrNull() ?: return
        val id = request.optString("id"); if (id.length !in 1..128) return
        val command = request.optString("command"); val args = request.optJSONObject("args") ?: JSONObject()
        if (command == "pick_folder") { activity.runOnUiThread { activity.pickFolder { uri, error -> reply(id, error == null, error ?: uri) } }; return }
        val executor = if (command in listOf("get_state", "save_state", "set_running", "get_capabilities", "google_cancel")) control else io
        executor.execute {
            try {
                val repo = MediaRepository(activity.applicationContext, store)
                val value: Any? = when (command) {
                    "get_state" -> store.state(true)
                    "save_state" -> { val old = store.state(); val state = store.save(args.getJSONObject("state")); WallpaperWorker.schedule(activity); io.execute { runCatching { repo.cleanup(old, store.state()) } }; state }
                    "set_running" -> { val state = store.setRunning(args.getBoolean("running")); WallpaperWorker.schedule(activity); state }
                    "list_media" -> JSONArray().apply { repo.list().forEach { put(it.json(true)) } }
                    "get_preview" -> { val mediaId = args.getString("id"); val media = repo.list().find { it.id == mediaId } ?: error("Image no longer available"); repo.visual(media); "${TrustedWebView.origin}/media/${media.id}.img" }
                    "next_wallpaper" -> try { WallpaperEngine.apply(activity, true) } catch (e: Exception) { store.mutateState { it.put("error", e.message ?: "Wallpaper update failed") }; throw e }
                    "get_capabilities" -> capabilities()
                    "clear_cache" -> { repo.clear(); JSONObject.NULL }
                    "google_begin" -> { authorizationIds.add(args.getString("sourceId")); GoogleAmbient(activity, store).begin(args) }
                    "google_poll" -> GoogleAmbient(activity, store).poll(args.getString("sourceId"))
                    "google_disconnect" -> { GoogleAmbient(activity, store).disconnect(args.getString("sourceId")); JSONObject.NULL }
                    "google_cancel" -> { GoogleAmbient(activity, store).cancel(args.getString("sourceId")); repo.releaseUnusedGrants(); JSONObject.NULL }
                    "open_external" -> { val uri = Uri.parse(args.getString("url")); require(uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null) { "Only HTTPS external links are supported" }; activity.runOnUiThread { try { activity.startActivity(Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE)); reply(id, true, null) } catch (e: Exception) { reply(id, false, "No browser is installed on this device. Scan the QR code on your phone.") } }; return@execute }
                    "open_screensaver_settings" -> { activity.runOnUiThread { try { activity.startActivity(Intent(Settings.ACTION_DREAM_SETTINGS)); reply(id, true, null) } catch (e: Exception) { reply(id, false, "This firmware hides Screensaver settings. Open TV Settings → Device Preferences → Screensaver and choose Wallta.") } }; return@execute }
                    else -> error("Unknown native command: $command")
                }
                reply(id, true, value)
            } catch (e: Exception) { reply(id, false, e.message ?: "Native operation failed") }
        }
    }
    private fun capabilities(): JSONObject {
        val tv = activity.getSystemService(UiModeManager::class.java).currentModeType == Configuration.UI_MODE_TYPE_TELEVISION
        val wm = WallpaperManager.getInstance(activity)
        val wallpaper = wm.isWallpaperSupported && wm.isSetWallpaperAllowed
        return JSONObject().put("platform", if (tv) "android-tv" else "android").put("wallpaper", wallpaper).put("screensaver", true).put("minIntervalSeconds", 900)
            .put("notes", JSONArray().put("Android schedules wallpaper changes at a minimum of 15 minutes; Doze and OEM power rules can delay execution. Screensaver slides use their own shorter interval.")
                .put(if (tv) "Home wallpaper depends on TV firmware and launcher. Even when the Android API reports support, visually confirm it on Home. Wallta does not replace your launcher." else "Choose Wallta in Android Screensaver settings to enable its slideshow.")
                .put("A firmware DocumentsProvider is needed to choose local or USB folders. USB access resumes only when the same storage remains mounted."))
    }
    private fun reply(id: String, success: Boolean, value: Any?) {
        if (closed) return
        val json = when (value) { null, JSONObject.NULL -> "null"; is JSONObject, is JSONArray -> value.toString(); is Number, is Boolean -> value.toString(); else -> JSONObject.quote(value.toString()) }
        activity.runOnUiThread { if (!closed) web.evaluateJavascript("window.__walltaReply&&window.__walltaReply(${JSONObject.quote(id)},$success,$json)", null) }
    }
    fun close() { closed = true; authorizationIds.forEach { GoogleAmbient(activity, store).cancel(it) }; runCatching { MediaRepository(activity.applicationContext, store).releaseUnusedGrants() }; io.shutdownNow(); control.shutdownNow() }
}
