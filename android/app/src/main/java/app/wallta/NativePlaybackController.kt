package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Movie
import android.os.Handler
import android.os.Looper
import android.view.SurfaceHolder
import org.json.JSONObject
import java.io.File
import java.time.Instant
import java.util.concurrent.Executors
import java.util.concurrent.Future

/** Used by WallpaperService and DreamService; no overlays, web views or remote codec URLs. */
internal class NativePlaybackController(private val context: Context, private val holder: SurfaceHolder, private val dream: Boolean, private val preview: Boolean = false, private val status: (String, JSONObject) -> Unit = { _, _ -> }) {
    private val store = StateStore(context)
    private val handler = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private val renderer = NativeMediaSurface(holder)
    private val tag = "motion:${java.util.UUID.randomUUID()}"
    private var work: Future<*>? = null
    @Volatile private var active = false
    @Volatile private var generation = 0L
    private var revision = -1L
    private var shown: Media? = null
    private var visible: Frame? = null
    private var nextSlide = 0L
    private var settings = JSONObject()
    private val pending = mutableSetOf<Frame>()
    private class Frame(val media: Media, val file: File, val kind: String, val image: Bitmap?, val animation: Movie?) {
        fun discard() { image?.takeIf { !it.isRecycled }?.recycle(); ActiveMediaPins.release(media.id) }
    }
    fun start() {
        if (active || io.isShutdown || !holder.surface.isValid) return
        active = true; generation++; handler.post(tick)
    }
    private val tick = object : Runnable {
        override fun run() {
            if (!active) return
            val current = store.state(); val rev = store.revision()
            val media = shown
            val invalid = media != null && (!MotionPolicy.available(current, media, dream) || (media.photos && !MediaRepository(context, store).networkAllowed(current.getJSONObject("settings"))))
            val selectedChanged = !dream && media != null && current.optJSONObject("current")?.optString("id")?.let { it != media.id } == true
            if (invalid || revision != -1L && revision != rev || selectedChanged) { cancel(); renderer.stop(clear = true); visible?.discard(); visible = null; shown = null; revision = -1L; nextSlide = 0 }
            if ((shown == null || dream && System.currentTimeMillis() >= nextSlide) && work == null && System.currentTimeMillis() >= nextSlide) load()
            handler.postDelayed(this, 1000)
        }
    }
    private fun load() {
        val session = generation
        work = io.submit {
            val snapshot = synchronized(StateStore.lock) { store.state() to store.revision() }
            val state = snapshot.first; val selectedSettings = state.getJSONObject("settings"); val selectedRevision = snapshot.second
            fun ensureCurrent() { check(active && generation == session && !Thread.currentThread().isInterrupted && store.revision() == selectedRevision) { "Playback cancelled" } }
            try {
                check(WallpaperEngine.allowed(selectedSettings)) { "Wallta is outside the configured schedule" }
                val repo = MediaRepository(context, store)
                val homeSource = selectedSettings.optString("homeSourceId").takeIf { it.isNotBlank() }
                val items = repo.list(sourceIds = if (dream || homeSource == null) null else setOf(homeSource))
                check(items.isNotEmpty()) { "Add a media source in Wallta" }
                fun prepare(media: Media): Frame {
                    ActiveMediaPins.acquire(media.id)
                    try {
                    val raw = repo.image(media); val detected = MotionPolicy.fileKind(raw)
                    val animated = selectedSettings.optBoolean("animatedWallpaper")
                    if (animated && detected == "video") return Frame(media, raw, "video", null, null)
                    if (animated && detected == "gif") {
                        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(raw.path, bounds)
                        check(bounds.outWidth in 1..4096 && bounds.outHeight in 1..4096 && bounds.outWidth.toLong() * bounds.outHeight <= 8_388_608) { "Animated GIF exceeds 8 MP playback limit" }
                        val movie = Movie.decodeFile(raw.path) ?: error("Cannot decode GIF animation")
                        return Frame(media, raw, "gif", null, movie)
                    }
                    val display = context.resources.displayMetrics
                    val width = holder.surfaceFrame.width().takeIf { it > 0 } ?: display.widthPixels
                    val height = holder.surfaceFrame.height().takeIf { it > 0 } ?: display.heightPixels
                    val bitmap = WallpaperEngine.render(raw.path, selectedSettings.optString("fit", "fill"), width.coerceIn(1, 3840), height.coerceIn(1, 3840), selectedSettings.optBoolean("blurBackground"), detected == "video")
                    return Frame(media, raw, "image", bitmap, null)
                    } catch (e: Throwable) { ActiveMediaPins.release(media.id); throw e }
                }
                val loaded = Net.scoped(tag) {
                    val current = if (dream) null else items.find { it.id == state.optJSONObject("current")?.optString("id") }
                    if (current != null) current to prepare(current)
                    else Rotation.loadUsable(store, items, selectedSettings, if (dream) "dreamRotation" else "rotation", ::ensureCurrent, { frame: Frame -> frame.discard() }, ::prepare)
                }
                val frame = loaded.second
                synchronized(pending) {
                    if (!active || generation != session) { frame.discard(); return@submit }
                    pending += frame
                    handler.post publish@{
                        synchronized(pending) {
                            if (!pending.remove(frame)) return@publish
                            if (!active || generation != session || store.revision() != selectedRevision) { frame.discard(); work = null; return@publish }
                            settings = selectedSettings; revision = selectedRevision; shown = frame.media; work = null
                            renderer.stop(); visible?.discard(); visible = frame
                            nextSlide = if (dream) System.currentTimeMillis() + selectedSettings.optLong("slideshowSeconds", 30).coerceIn(5, 86400) * 1000 else 0L
                            renderer.show(frame.file, frame.kind, frame.image, frame.animation, selectedSettings.optString("fit", "fill"), ready = ready@{
                                if (!active || generation != session || store.revision() != selectedRevision) { renderer.stop(clear = true); shown = null; return@ready }
                                if (dream) Rotation.consumed(store, items, frame.media, "dreamRotation")
                                else if (!preview) {
                                    Rotation.consumed(store, items, frame.media)
                                    store.mutateState { it.put("current", frame.media.json()).put("lastChanged", Instant.now().toString()).put("error", JSONObject.NULL) }
                                }
                                status("", selectedSettings)
                            }, error = { error ->
                                status(error, selectedSettings)
                                if (!dream && !preview) store.mutateState { it.put("error", error) }
                            })
                        }
                    }
                }
            } catch (e: Exception) {
                if (active && generation == session && !Thread.currentThread().isInterrupted) handler.post {
                    if (active && generation == session) { work = null; nextSlide = System.currentTimeMillis() + 5000; status(e.message ?: "Cannot play media", selectedSettings) }
                }
            }
        }
    }
    private fun cancel() { generation++; work?.cancel(true); work = null; Net.cancel(tag); synchronized(pending) { pending.forEach { it.discard() }; pending.clear() } }
    fun stop() { active = false; cancel(); handler.removeCallbacksAndMessages(null); renderer.stop(clear = true); visible?.discard(); visible = null; shown = null; revision = -1L; nextSlide = 0 }
    fun destroy() { stop(); io.shutdownNow() }
}
