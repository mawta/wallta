package app.wallta

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.drawable.BitmapDrawable
import android.os.Handler
import android.os.Looper
import android.service.dreams.DreamService
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import org.json.JSONObject
import java.text.DateFormat
import java.util.Date
import java.util.concurrent.Executors
import java.util.concurrent.Future

/** Native renderer: the background screensaver never creates a WebView or React UI. */
class WalltaDreamService : DreamService() {
    private val handler = Handler(Looper.getMainLooper())
    private val lifecycle = Any()
    private var io = Executors.newSingleThreadExecutor()
    private var work: Future<*>? = null
    private val requestTag = "dream:${java.util.UUID.randomUUID()}"
    @Volatile private var active = false
    @Volatile private var generation = 0L
    private var image: ImageView? = null
    private var background: ImageView? = null
    private var clock: TextView? = null
    private var message: TextView? = null
    private var visible: DreamFrame? = null
    private var visibleSettings = JSONObject()
    private var visiblePhotos = false
    private var visibleRevision = -1L
    // Frames posted from I/O are tracked so removing callbacks at stop cannot leak them.
    private val pending = mutableSetOf<DreamFrame>()

    private class DreamFrame(val image: Bitmap, val background: Bitmap?) {
        fun recycle() { background?.takeIf { it !== image }?.recycle(); image.recycle() }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        isInteractive = false; isFullscreen = true; isScreenBright = true
        val root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        val full = FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT)
        background = ImageView(this).apply { scaleType = ImageView.ScaleType.CENTER_CROP; imageAlpha = 150; visibility = View.GONE }
        image = ImageView(this).apply { scaleType = ImageView.ScaleType.CENTER_CROP }
        root.addView(background, full); root.addView(image, FrameLayout.LayoutParams(full))
        val inset = (32 * resources.displayMetrics.density).toInt()
        clock = TextView(this).apply { setTextColor(Color.WHITE); textSize = 42f; setShadowLayer(5f, 0f, 2f, Color.BLACK); visibility = View.GONE }
        message = TextView(this).apply { setTextColor(Color.WHITE); textSize = 24f; text = "Wallta · Loading photos…"; setShadowLayer(4f, 0f, 2f, Color.BLACK) }
        root.addView(clock, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.END).apply { setMargins(inset, inset, inset, inset) })
        root.addView(message, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.START).apply { setMargins(inset, inset, inset, inset) })
        setContentView(root)
    }

    override fun onDreamingStarted() {
        super.onDreamingStarted()
        synchronized(lifecycle) { generation++; active = true }
        if (io.isShutdown) io = Executors.newSingleThreadExecutor()
        handler.post(clockTick); handler.post(slide)
    }

    private val clockTick = object : Runnable {
        override fun run() {
            if (!active) return
            clock?.text = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date())
            // Ambient content must disappear when offline, even between slide changes.
            if (visiblePhotos && (!MediaRepository(applicationContext).networkAllowed(visibleSettings) || StateStore(applicationContext).revision() != visibleRevision)) showError("Google Photos source changed or is offline")
            handler.postDelayed(this, 1000)
        }
    }

    private val slide = object : Runnable {
        override fun run() {
            if (!active) return
            val session = generation
            work = io.submit {
                val store = StateStore(applicationContext)
                val snapshot = synchronized(StateStore.lock) { store.state().getJSONObject("settings") to store.revision() }
                val settings = snapshot.first; val revision = snapshot.second
                fun ensureCurrent() { check(active && generation == session && !Thread.currentThread().isInterrupted && store.revision() == revision) { "Screensaver operation cancelled" } }
                try {
                    check(WallpaperEngine.allowed(settings)) { "Wallta is outside the configured schedule" }
                    val repo = MediaRepository(applicationContext, store)
                    var items = emptyList<Media>()
                    val loaded = Net.scoped(requestTag) {
                        items = repo.list()
                        Rotation.loadUsable(store, items, settings, "dreamRotation", ::ensureCurrent, discard = { frame: DreamFrame -> frame.recycle() }) { candidate ->
                            val file = repo.visual(candidate)
                            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(file.path, bounds)
                            check(bounds.outWidth in 1..2048 && bounds.outHeight in 1..2048) { "Screensaver preview exceeds decode limits" }
                            val bitmap = BitmapFactory.decodeFile(file.path) ?: error("Cannot decode screensaver preview")
                            try {
                                val blurred = if (settings.optBoolean("blurBackground") && settings.optString("fit") == "fit") {
                                    val longest = maxOf(bitmap.width, bitmap.height)
                                    Bitmap.createScaledBitmap(bitmap, maxOf(1, bitmap.width * 24 / longest), maxOf(1, bitmap.height * 24 / longest), true)
                                } else null
                                DreamFrame(bitmap, blurred)
                            } catch (e: Exception) { bitmap.recycle(); throw e }
                        }
                    }
                    val media = loaded.first; val frame = loaded.second
                    synchronized(lifecycle) {
                        if (!active || generation != session || Thread.currentThread().isInterrupted) frame.recycle()
                        else {
                            pending += frame
                            handler.post {
                                synchronized(lifecycle) publish@{
                                    if (!pending.remove(frame)) return@publish
                                    if (!active || generation != session || image == null) { frame.recycle(); return@publish }
                                    synchronized(StateStore.lock) commit@{
                                        if (store.revision() != revision || (media.photos && !repo.networkAllowed(settings))) { frame.recycle(); return@commit }
                                        showFrame(frame, settings, media.photos, revision)
                                        runCatching { Rotation.consumed(store, items, media, "dreamRotation") }.onFailure { message?.text = "Could not save screensaver rotation state" }
                                    }
                                }
                            }
                        }
                    }
                } catch (e: Exception) {
                    synchronized(lifecycle) {
                        if (active && generation == session && !Thread.currentThread().isInterrupted) handler.post { if (active && generation == session) showError(e.message ?: "Add an image source in Wallta") }
                    }
                } finally {
                    synchronized(lifecycle) { if (active && generation == session) handler.postDelayed(this, settings.optLong("slideshowSeconds", 30).coerceIn(5, 86400) * 1000) }
                }
            }
        }
    }

    /** UI thread owns every published frame and releases the previous one after detaching it. */
    private fun showFrame(frame: DreamFrame, settings: JSONObject, photos: Boolean, revision: Long) {
        val previous = visible
        image?.scaleType = when (settings.optString("fit")) { "fit" -> ImageView.ScaleType.FIT_CENTER; "stretch" -> ImageView.ScaleType.FIT_XY; else -> ImageView.ScaleType.CENTER_CROP }
        image?.setImageDrawable(BitmapDrawable(resources, frame.image).apply { isFilterBitmap = true })
        background?.setImageDrawable(frame.background?.let { BitmapDrawable(resources, it).apply { isFilterBitmap = true } })
        background?.visibility = if (frame.background == null) View.GONE else View.VISIBLE
        clock?.visibility = if (settings.optBoolean("showClock")) View.VISIBLE else View.GONE
        message?.text = ""
        visible = frame; visibleSettings = settings; visiblePhotos = photos; visibleRevision = revision
        previous?.recycle()
    }
    private fun clearFrame() {
        image?.setImageDrawable(null); background?.setImageDrawable(null); background?.visibility = View.GONE
        visible?.recycle(); visible = null; visiblePhotos = false; clock?.visibility = View.GONE
    }
    private fun showError(text: String) { clearFrame(); message?.text = text }
    private fun stop() {
        synchronized(lifecycle) {
            active = false; generation++
            handler.removeCallbacksAndMessages(null); work?.cancel(true); Net.cancel(requestTag); work = null; io.shutdownNow()
            pending.forEach { it.recycle() }; pending.clear()
            clearFrame()
        }
    }
    override fun onDreamingStopped() { stop(); super.onDreamingStopped() }
    override fun onDetachedFromWindow() { stop(); image = null; background = null; clock = null; message = null; super.onDetachedFromWindow() }
}
