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
import kotlinx.coroutines.runInterruptible
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
    private val lockScreenApply = Any()

    fun allowed(settings: JSONObject) = Policy.inSchedule(settings.optBoolean("scheduleEnabled"), settings.optJSONArray("days")?.let { a -> (0 until a.length()).map { a.getInt(it) }.toSet() } ?: emptySet(), settings.optString("startTime", "07:00"), settings.optString("endTime", "23:00"), LocalDateTime.now())
    fun apply(context: Context, manual: Boolean = false, sourceId: String? = null, startRunning: Boolean = false): JSONObject =
        applyTo(context, manual, WallpaperManager.FLAG_SYSTEM, sourceId, startRunning)

    fun applyLock(context: Context, manual: Boolean = false): JSONObject = synchronized(lockScreenApply) {
        try { applyTo(context, manual, WallpaperManager.FLAG_LOCK) }
        catch (e: Exception) { if (!Thread.currentThread().isInterrupted) StateStore(context).mutateState { it.put("lockError", e.message ?: "Lock wallpaper update failed") }; throw e }
    }

    private fun applyTo(context: Context, manual: Boolean, target: Int, sourceId: String? = null, startRunning: Boolean = false): JSONObject {
        val store = StateStore(context); val snapshot = synchronized(StateStore.lock) { store.state() to store.revision() }
        val initial = snapshot.first; val revision = snapshot.second; val settings = initial.getJSONObject("settings")
        val lockScreen = target == WallpaperManager.FLAG_LOCK
        if (!manual && (!(if (lockScreen) settings.optBoolean("lockRunning") else initial.optBoolean("running")) || (!lockScreen && !allowed(settings)))) return store.state(true)
        val lockSource = if (lockScreen) StateStore.lockSourceId(initial) else null
        if (lockScreen && lockSource == null) {
            require(!manual) { "Choose a lock screen source first." }
            return store.state(true)
        }
        val rotationKey = if (lockScreen) "lockRotation" else "rotation"
        val manager = WallpaperManager.getInstance(context)
        require(!lockScreen || NativeWallpaperSupport.lock(context)) { "This TV or firmware does not provide a supported Lock screen. Use Wallta in Screensaver settings." }
        require(manager.isWallpaperSupported && manager.isSetWallpaperAllowed) {
            if (lockScreen) "This device does not allow lock screen wallpaper changes."
            else "Firmware does not permit changing the Home wallpaper. Use Wallta in Screensaver settings."
        }
        fun ensureCurrent() = synchronized(StateStore.lock) { check(store.revision() == revision && (manual || (if (lockScreen) store.state().getJSONObject("settings").optBoolean("lockRunning") else store.state().optBoolean("running"))) && !Thread.currentThread().isInterrupted) { "Wallpaper operation cancelled by a newer setting" } }
        val repo = MediaRepository(context, store)
        val selectedSource = sourceId ?: settings.optString("homeSourceId").takeIf { it.isNotBlank() }
        val items = if (lockSource != null) repo.list(sourceIds = setOf(lockSource)) else MediaSafety.forSource(repo.list(), selectedSource)
        require(items.isNotEmpty()) { "Thư mục đã chọn chưa có ảnh khả dụng." }
        // An installed live engine owns Home. A static setBitmap would silently disable it.
        if (!lockScreen && NativeWallpaperSupport.selected(context)) {
            val loaded = Rotation.loadUsable(store, items, settings, ensureCurrent = ::ensureCurrent) { candidate -> repo.image(candidate) }
            synchronized(StateStore.lock) {
                ensureCurrent(); check(NativeWallpaperSupport.selected(context)) { "Live wallpaper selection changed" }
                Rotation.consumed(store, items, loaded.first)
                return store.mutateState { it.put("current", loaded.first.json()).put("error", JSONObject.NULL); if (startRunning) it.put("running", true) }
            }
        }
        val loaded = Rotation.loadUsable(store, items, settings, key = rotationKey, ensureCurrent = ::ensureCurrent, discard = { bitmap: Bitmap -> bitmap.recycle() }) { candidate ->
            val original = repo.image(candidate)
            require(lockScreen || !settings.optBoolean("animatedWallpaper") || MotionPolicy.fileKind(original) == "image") { "Confirm Wallta in Android Live Wallpaper settings to play animated Home wallpaper." }
            render(original.absolutePath, settings.optString("fit", "fill"), manager.desiredMinimumWidth.coerceIn(720, 2560), manager.desiredMinimumHeight.coerceIn(720, 2560), settings.optBoolean("blurBackground"), MotionPolicy.fileKind(original) == "video")
        }
        val media = loaded.first; val bitmap = loaded.second
        try {
            synchronized(StateStore.lock) {
                ensureCurrent()
                val result = manager.setBitmap(bitmap, null, true, target)
                check(result != 0) { "Firmware rejected the wallpaper" }
                Rotation.consumed(store, items, media, key = rotationKey)
                if (lockScreen) return store.mutateState { it.put("lockCurrent", media.json()).put("lockLastChanged", Instant.now().toString()).put("lockError", JSONObject.NULL) }
                return store.mutateState { it.put("current", media.json()).put("lastChanged", Instant.now().toString()).put("error", JSONObject.NULL); if (startRunning) it.put("running", true) }
            }
        } finally { bitmap.recycle() }
    }
    internal fun render(path: String, fit: String, width: Int, height: Int, blur: Boolean, video: Boolean = false): Bitmap {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }; BitmapFactory.decodeFile(path, bounds)
        var sample = 1
        while (bounds.outWidth / sample > 4096 || bounds.outHeight / sample > 4096 || (bounds.outWidth.toLong() / sample * bounds.outHeight / sample) > 16_777_216) sample *= 2
        val source = if (video) MotionPoster.frame(java.io.File(path), width, height) else BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: error("Cannot decode image")
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
        private const val lockName = "wallta-lock-wallpaper"
        fun schedule(context: Context): Unit = synchronized(StateStore.lock) {
            val state = StateStore(context).state(); val wm = WorkManager.getInstance(context)
            val running = state.optBoolean("running")
            if (!running) wm.cancelUniqueWork(name)
            val rotateLock = state.getJSONObject("settings").optBoolean("lockRunning") && StateStore.lockSourceId(state) != null && NativeWallpaperSupport.lock(context)
            if (!rotateLock) wm.cancelUniqueWork(lockName)
            val seconds = state.getJSONObject("settings").optLong("intervalSeconds", 900).coerceAtLeast(900)
            if (running) {
                val work = PeriodicWorkRequestBuilder<WallpaperWorker>(seconds, TimeUnit.SECONDS).setInitialDelay(seconds, TimeUnit.SECONDS).build()
                wm.enqueueUniquePeriodicWork(name, ExistingPeriodicWorkPolicy.UPDATE, work)
            }
            if (rotateLock) {
                val lockSeconds = state.getJSONObject("settings").optLong("lockIntervalSeconds", 900).coerceAtLeast(900)
                val lockWork = PeriodicWorkRequestBuilder<LockWallpaperWorker>(lockSeconds, TimeUnit.SECONDS).setInitialDelay(lockSeconds, TimeUnit.SECONDS).build()
                wm.enqueueUniquePeriodicWork(lockName, ExistingPeriodicWorkPolicy.UPDATE, lockWork)
            }
        }
    }
}

class LockWallpaperWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        try {
            runInterruptible(Dispatchers.IO) { WallpaperEngine.applyLock(applicationContext) }
            return Result.success()
        }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (e: Exception) { StateStore(applicationContext).mutateState { it.put("lockError", e.message ?: "Lock wallpaper update failed") }; return if (runAttemptCount < 2) Result.retry() else Result.success() }
    }
}
