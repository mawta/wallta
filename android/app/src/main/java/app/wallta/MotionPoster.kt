package app.wallta

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.os.Build
import java.io.File

/** A native decoder creates bounded JPEGs; raw video never enters WebView/image decoders. */
internal object MotionPoster {
    private val decodeLock = Any()
    fun frame(original: File, width: Int, height: Int): Bitmap = synchronized(decodeLock) {
        val dimensions = validate(original)
        require(Build.VERSION.SDK_INT >= 27 || dimensions.first.toLong() * dimensions.second <= 4_194_304) { "Video posters above 4 MP require Android 8.1 or later" }
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(original.path)
            (if (Build.VERSION.SDK_INT >= 27) retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, width, height) else retriever.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)) ?: error("Native codec cannot decode this video")
        } finally { retriever.release() }
    }
    fun validate(file: File): Pair<Int, Int> {
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(file.path)
            val width = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
            val height = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
            require(width in 1..8192 && height in 1..8192 && width.toLong() * height <= 33_554_432) { "Unsupported or oversized video" }
            return width to height
        } finally { retriever.release() }
    }
    fun prepare(context: Context, store: StateStore, media: Media, original: File, snapshot: Pair<Long, Long>): File = synchronized(decodeLock) {
        val target = VisualCache.file(context, media.id)
        fun ensureCurrent() { check(!Thread.currentThread().isInterrupted && store.revision() == snapshot.first && (store.get("cacheEpoch")?.optLong("value") ?: 0L) == snapshot.second && original.isFile) { "Video preview cancelled" } }
        synchronized(StateStore.lock) { ensureCurrent(); if (target.isFile) return target }
        val dimensions = validate(original)
        // Android 8.0 lacks scaled extraction; refuse large full-frame decode there.
        require(Build.VERSION.SDK_INT >= 27 || dimensions.first.toLong() * dimensions.second <= 4_194_304) { "Video posters above 4 MP require Android 8.1 or later" }
        val retriever = MediaMetadataRetriever()
        var frame: Bitmap? = null
        var scaled: Bitmap? = null
        val temporary = File.createTempFile("poster-", ".tmp", target.parentFile)
        try {
            retriever.setDataSource(original.path)
            val size = MediaSafety.visualSize(dimensions.first, dimensions.second)
            frame = if (Build.VERSION.SDK_INT >= 27) retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, size.first, size.second) else retriever.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
            val bitmap = frame ?: error("Native codec cannot decode this video")
            val bound = MediaSafety.visualSize(bitmap.width, bitmap.height)
            scaled = if (bitmap.width == bound.first && bitmap.height == bound.second) bitmap else Bitmap.createScaledBitmap(bitmap, bound.first, bound.second, true)
            temporary.outputStream().use { check(scaled.compress(Bitmap.CompressFormat.JPEG, 90, it)) }
            synchronized(StateStore.lock) { ensureCurrent(); check(temporary.renameTo(target)) { "Cannot cache video poster" } }
            target
        } finally { temporary.delete(); scaled?.takeIf { it !== frame }?.recycle(); frame?.recycle(); retriever.release() }
    }
}
