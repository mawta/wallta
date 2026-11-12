package app.wallta

import android.graphics.Color
import android.os.Handler
import android.os.Looper
import android.service.dreams.DreamService
import android.view.Gravity
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import java.text.DateFormat
import java.util.Date

/** Firmware-owned native screensaver surface, sharing visible-only motion playback. */
class WalltaDreamService : DreamService() {
    private var playback: NativePlaybackController? = null
    private var surface: SurfaceView? = null
    private var message: TextView? = null
    private var clock: TextView? = null
    private var showClock = false
    private var dreaming = false
    private val handler = Handler(Looper.getMainLooper())
    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        isInteractive = false; isFullscreen = true; isScreenBright = true
        val root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        surface = SurfaceView(this)
        root.addView(surface, FrameLayout.LayoutParams(-1, -1))
        val inset = (32 * resources.displayMetrics.density).toInt()
        message = TextView(this).apply { text = "Wallta · Loading media…"; setTextColor(Color.WHITE); textSize = 24f; setShadowLayer(4f, 0f, 2f, Color.BLACK) }
        clock = TextView(this).apply { setTextColor(Color.WHITE); textSize = 42f; setShadowLayer(5f, 0f, 2f, Color.BLACK); visibility = View.GONE }
        root.addView(message, FrameLayout.LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.START).apply { setMargins(inset, inset, inset, inset) })
        root.addView(clock, FrameLayout.LayoutParams(-2, -2, Gravity.BOTTOM or Gravity.END).apply { setMargins(inset, inset, inset, inset) })
        surface?.holder?.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                playback = NativePlaybackController(applicationContext, holder, dream = true) { text, settings ->
                    message?.text = text; showClock = settings.optBoolean("showClock")
                    clock?.visibility = if (showClock && text.isEmpty()) View.VISIBLE else View.GONE
                }
                if (dreaming) playback?.start()
            }
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) { playback?.stop(); if (dreaming) playback?.start() }
            override fun surfaceDestroyed(holder: SurfaceHolder) { playback?.destroy(); playback = null }
        })
        setContentView(root)
    }
    private val tick = object : Runnable {
        override fun run() { if (!dreaming) return; clock?.text = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date()); handler.postDelayed(this, 1000) }
    }
    override fun onDreamingStarted() { super.onDreamingStarted(); dreaming = true; playback?.start(); handler.post(tick) }
    override fun onDreamingStopped() { dreaming = false; handler.removeCallbacksAndMessages(null); playback?.stop(); super.onDreamingStopped() }
    override fun onDetachedFromWindow() { dreaming = false; handler.removeCallbacksAndMessages(null); playback?.destroy(); playback = null; surface = null; message = null; clock = null; super.onDetachedFromWindow() }
}
