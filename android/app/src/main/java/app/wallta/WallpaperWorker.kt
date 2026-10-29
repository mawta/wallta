package app.wallta

import android.app.WallpaperManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant
import java.time.LocalDateTime
import java.util.concurrent.TimeUnit

object Rotation {
    fun <T> loadUsable(store: StateStore, items: List<Media>, settings: JSONObject, key: String = "rotation", ensureCurrent: () -> Unit, discard: (T) -> Unit = {}, load: (Media) -> T): Pair<Media, T> {
        val r = store.get(key) ?: JSONObject(); val used = r.optJSONArray("used") ?: JSONArray()
        val consumed = (0 until used.length()).map { used.getString(it) }.toSet()
        val byId = items.associateBy { it.id }
        val loaded = MediaSafety.firstUsable(items.map { it.id }, consumed, settings.optString("order") == "shuffle", r.optString("current"), ensureCurrent, discard) { id -> load(byId.getValue(id)) }
        return byId.getValue(loaded.first) to loaded.second
    }
    fun choose(store: StateStore, items: List<Media>, settings: JSONObject, key: String = "rotation"): Media {
        val r = store.get(key) ?: JSONObject(); val used = r.optJSONArray("used") ?: JSONArray()
        val ids = items.map { it.id }; val consumed = (0 until used.length()).map { used.getString(it) }.toSet().intersect(ids.toSet())
        val id = Policy.next(ids, consumed, settings.optString("order") == "shuffle", r.optString("current")) ?: error("No images available")
        return items.first { it.id == id }
    }
    fun consumed(store: StateStore, items: List<Media>, media: Media, key: String = "rotation") {
        val r = store.get(key) ?: JSONObject(); val a = r.optJSONArray("used") ?: JSONArray(); val ids = items.map { it.id }.toSet()
        var used = (0 until a.length()).map { a.getString(it) }.toSet().intersect(ids)
        if (used.containsAll(ids)) used = emptySet()
        store.put(key, JSONObject().put("current", media.id).put("used", JSONArray((used + media.id).toList())))
    }
}

object WallpaperEngine {
    fun allowed(settings: JSONObject) = Policy.inSchedule(settings.optBoolean("scheduleEnabled"), settings.optJSONArray("days")?.let { a -> (0 until a.length()).map { a.getInt(it) }.toSet() } ?: emptySet(), settings.optString("startTime", "07:00"), settings.optString("endTime", "23:00"), LocalDateTime.now())
    fun apply(context: Context, manual: Boolean = false, sourceId: String? = null, startRunning: Boolean = false): JSONObject {
        val store = StateStore(context); val snapshot = synchronized(StateStore.lock) { store.state() to store.revision() }
        val initial = snapshot.first; val revision = snapshot.second; val settings = initial.getJSONObject("settings")
        if (!manual && (!initial.optBoolean("running") || !allowed(settings))) return store.state(true)
        val manager = WallpaperManager.getInstance(context)
        require(manager.isWallpaperSupported && manager.isSetWallpaperAllowed) { "Firmware does not permit changing the Home wallpaper. Use Wallta in Screensaver settings." }
        fun ensureCurrent() = synchronized(StateStore.lock) { check(store.revision() == revision && (manual || store.state().optBoolean("running")) && !Thread.currentThread().isInterrupted) { "Wallpaper operation cancelled by a newer setting" } }
        val repo = MediaRepository(context, store); val items = MediaSafety.forSource(repo.list(), sourceId)
        require(items.isNotEmpty()) { "Thư mục đã chọn chưa có ảnh khả dụng." }
        val loaded = Rotation.loadUsable(store, items, settings, ensureCurrent = ::ensureCurrent, discard = { bitmap: Bitmap -> bitmap.recycle() }) { candidate ->
            val file = repo.image(candidate)
            render(file.absolutePath, settings.optString("fit", "fill"), manager.desiredMinimumWidth.coerceIn(720, 2560), manager.desiredMinimumHeight.coerceIn(720, 2560), settings.optBoolean("blurBackground"))
        }
        val media = loaded.first; val bitmap = loaded.second
        try {
            synchronized(StateStore.lock) {
                ensureCurrent()
                val result = manager.setBitmap(bitmap, null, true, WallpaperManager.FLAG_SYSTEM)
                check(result != 0) { "Firmware rejected the wallpaper" }
                Rotation.consumed(store, items, media)
                return store.mutateState { it.put("current", media.json()).put("lastChanged", Instant.now().toString()).put("error", JSONObject.NULL); if (startRunning) it.put("running", true) }
            }
        } finally { bitmap.recycle() }
    }
    private fun render(path: String, fit: String, width: Int, height: Int, blur: Boolean): Bitmap {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(path, bounds)
        var sample = 1
        while (bounds.outWidth / sample > 4096 || bounds.outHeight / sample > 4096 || (bounds.outWidth.toLong() / sample * bounds.outHeight / sample) > 8_000_000) sample *= 2
        val source = BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: error("Cannot decode image")
        try {
            val output = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888); val canvas = Canvas(output); canvas.drawColor(Color.BLACK)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
            fun dest(fill: Boolean): Rect { val scale = if (fill) maxOf(width.toFloat() / source.width, height.toFloat() / source.height) else minOf(width.toFloat() / source.width, height.toFloat() / source.height); val w = (source.width * scale).toInt(); val h = (source.height * scale).toInt(); return Rect((width - w) / 2, (height - h) / 2, (width + w) / 2, (height + h) / 2) }
            if (blur && fit == "fit") {
                val tiny = Bitmap.createScaledBitmap(source, 24, maxOf(1, source.height * 24 / source.width).coerceAtMost(128), true)
                canvas.drawBitmap(tiny, null, dest(true), paint); tiny.recycle(); canvas.drawColor(0x66000000)
            }
            canvas.drawBitmap(source, null, if (fit == "stretch") Rect(0, 0, width, height) else dest(fit != "fit"), paint)
            return output
        } finally { source.recycle() }
    }
}

class WallpaperWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        try { WallpaperEngine.apply(applicationContext); Result.success() }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (e: Exception) { StateStore(applicationContext).mutateState { it.put("error", e.message ?: "Wallpaper update failed") }; Result.retry() }
    }
    companion object {
        private const val name = "wallta-wallpaper"
        fun schedule(context: Context) {
            val state = StateStore(context).state(); val wm = WorkManager.getInstance(context)
            if (!state.optBoolean("running")) { wm.cancelUniqueWork(name); return }
            val seconds = state.getJSONObject("settings").optLong("intervalSeconds", 900).coerceAtLeast(900)
            val work = PeriodicWorkRequestBuilder<WallpaperWorker>(seconds, TimeUnit.SECONDS).setInitialDelay(seconds, TimeUnit.SECONDS).build()
            wm.enqueueUniquePeriodicWork(name, ExistingPeriodicWorkPolicy.UPDATE, work)
        }
    }
}
