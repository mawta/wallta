package app.wallta

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Movie
import android.graphics.Paint
import android.graphics.RectF
import android.media.MediaPlayer
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.SurfaceHolder
import java.io.File

/** UI-thread owner for all decoders. stop() releases the codec and every draw callback. */
internal class NativeMediaSurface(private val holder: SurfaceHolder) {
    private val handler = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var bitmap: Bitmap? = null
    private var movie: Movie? = null
    private var started = 0L
    private var fit = "fill"
    private var failure: ((String) -> Unit)? = null
    fun show(file: File, kind: String, image: Bitmap?, animation: Movie?, fitMode: String, ready: () -> Unit, error: (String) -> Unit) {
        stop(); fit = fitMode; bitmap = image; movie = animation; failure = error
        if (!holder.surface.isValid) { stop(); error("Display surface is unavailable"); return }
        if (kind == "video") {
            val decoder = MediaPlayer(); player = decoder
            try {
                decoder.setSurface(holder.surface); decoder.setVolume(0f, 0f); decoder.isLooping = true
                decoder.setDataSource(file.path)
                decoder.setOnPreparedListener { media ->
                    if (player === media && holder.surface.isValid) {
                        try { media.setVideoScalingMode(if (fit == "fill") MediaPlayer.VIDEO_SCALING_MODE_SCALE_TO_FIT_WITH_CROPPING else MediaPlayer.VIDEO_SCALING_MODE_SCALE_TO_FIT); media.start(); ready() }
                        catch (e: Exception) { stop(); error(e.message ?: "Video playback failed") }
                    }
                }
                decoder.setOnErrorListener { _, _, _ -> stop(); error("Native codec cannot play this video"); true }
                decoder.prepareAsync()
            } catch (e: Exception) { stop(); error(e.message ?: "Video playback failed") }
        } else { started = SystemClock.uptimeMillis(); if (draw()) ready() }
    }
    private val redraw = Runnable { draw() }
    private fun draw(): Boolean {
        if (!holder.surface.isValid) return false
        var canvas: Canvas? = null
        try {
            canvas = holder.lockCanvas() ?: return false
            canvas.drawColor(Color.BLACK)
            val animation = movie; val still = bitmap
            val width = animation?.width() ?: still?.width ?: return false
            val height = animation?.height() ?: still?.height ?: return false
            val sx = canvas.width.toFloat() / width; val sy = canvas.height.toFloat() / height
            val scale = if (fit == "fit") minOf(sx, sy) else maxOf(sx, sy)
            val w = if (fit == "stretch") canvas.width.toFloat() else width * scale
            val h = if (fit == "stretch") canvas.height.toFloat() else height * scale
            val left = (canvas.width - w) / 2; val top = (canvas.height - h) / 2
            if (animation != null) {
                animation.setTime(((SystemClock.uptimeMillis() - started) % animation.duration().takeIf { it > 0 }.let { it ?: 1000 }).toInt())
                canvas.save(); canvas.translate(left, top); canvas.scale(w / width, h / height); animation.draw(canvas, 0f, 0f); canvas.restore()
            } else if (still != null) canvas.drawBitmap(still, null, RectF(left, top, left + w, top + h), Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG))
        } catch (e: Exception) { val error = failure; stop(); error?.invoke(e.message ?: "Display failed"); return false }
        finally { if (canvas != null) runCatching { holder.unlockCanvasAndPost(canvas) } }
        if (movie != null) handler.postDelayed(redraw, 42)
        return true
    }
    fun stop(clear: Boolean = false) {
        handler.removeCallbacksAndMessages(null)
        player?.let { decoder -> runCatching { decoder.setOnPreparedListener(null); decoder.setOnErrorListener(null); decoder.setSurface(null); decoder.release() } }; player = null
        movie = null; bitmap?.recycle(); bitmap = null; failure = null
        if (clear && holder.surface.isValid) {
            var canvas: Canvas? = null
            try { canvas = holder.lockCanvas(); canvas?.drawColor(Color.BLACK) }
            catch (_: Exception) {} finally { if (canvas != null) runCatching { holder.unlockCanvasAndPost(canvas) } }
        }
    }
}
