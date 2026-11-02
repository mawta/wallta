package app.wallta

import java.io.File
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import org.json.JSONArray
import org.json.JSONObject

/** One serial worker; state snapshots are short and never enter updater/store gates. */
class UpscaleManager(private val environment: Environment) : AutoCloseable {
    data class Output(val id: String, val name: String, val width: Int, val height: Int, val file: File, val before: File, val after: File, val sourceId: String? = null, val multipleSources: Boolean = false, val mediaId: String? = null, val publishedSourceId: String? = null)
    data class Publication(val source: JSONObject, val revision: Long, val mediaId: String? = null, val sourceId: String? = null)
    class ImageFailure(message: String, cause: Throwable? = null) : Exception(message, cause)
    interface Environment {
        val directory: File
        fun revision(): Long
        fun resolve(ids: List<String>): List<Media>
        fun snapshot(sourceId: String?): List<Media>
        fun input(media: Media): File
        fun render(input: File, output: File, before: File, after: File): Pair<Int, Int>
        /** Checks/rebases revision atomically with publication under updater/store gates. */
        fun append(jobId: String, output: Output, index: Int, revision: Long): Publication
        fun storage(): JSONObject = JSONObject().put("cacheBytes", 0).put("collections", JSONArray())
        fun clearCache(): JSONObject = storage()
        fun deleteCollection(collectionId: String): JSONObject = error("Collection unavailable")
        fun cancelWork() {}
    }
    private val worker = Executors.newSingleThreadExecutor()
    private var workerThread: Thread? = null
    private var closed = false
    private var running = false
    private var maintenance = false
    private var job: String? = null
    private var phase = "idle"
    private var revision = 0L
    private var total = 0
    private var completed = 0
    private var processed = 0
    private var failed = 0
    private var error: String? = null
    private var message: String? = null
    private val previews = mutableListOf<Output>()
    private var saved: JSONObject? = null

    fun start(ids: List<String>, mode: String, model: String? = null): JSONObject {
        UpscalePolicy.selection(ids, mode); UpscalePolicy.mode(mode, model)
        return begin(ids.toList(), null)
    }
    fun startAll(sourceId: String?, mode: String, model: String? = null): JSONObject {
        UpscalePolicy.mode(mode, model)
        require(sourceId == null || sourceId.length in 1..128) { "Invalid source ID" }
        return begin(null, sourceId)
    }
    private fun begin(ids: List<String>?, sourceId: String?): JSONObject {
        // Environment may acquire updater/StateStore locks. Do so outside this monitor.
        val expected = environment.revision()
        val id = UUID.randomUUID().toString()
        val previous = synchronized(this) {
            check(!closed && !running && !maintenance) { "A 4K job or storage operation is still running" }
            val old = job
            job = id; phase = "processing"; revision = expected; total = ids?.size ?: 0
            completed = 0; processed = 0; failed = 0; error = null; saved = null; previews.clear()
            message = "Preparing original images…"; running = true
            old
        }
        try { worker.execute { process(id, ids, sourceId, previous) } }
        catch (e: Exception) { synchronized(this) { running = false; phase = "error"; error = e.message }; throw e }
        return status()
    }
    private fun ensureCurrent(id: String) {
        val expected = synchronized(this) {
            check(!closed && job == id && phase == "processing" && !Thread.currentThread().isInterrupted) { "4K processing cancelled" }
            revision
        }
        check(environment.revision() == expected) { "Sources changed. Start 4K processing again." }
    }
    private fun process(id: String, ids: List<String>?, sourceId: String?, previous: String?) {
        val folder = File(environment.directory, id)
        var attempting = false
        synchronized(this) { workerThread = Thread.currentThread() }
        try {
            previous?.let { UpscaleFiles.removeChild(environment.directory, it) }
            ensureCurrent(id)
            UpscaleFiles.child(environment.directory, id, create = true)
            val selected = if (ids == null) environment.snapshot(sourceId) else environment.resolve(ids)
            ensureCurrent(id)
            check(ids == null || selected.map { it.id } == ids) { "Selected images are no longer available" }
            require(selected.isNotEmpty()) { "No enabled original images in this selection" }
            val multipleSources = selected.map { it.sourceId }.distinct().size > 1
            synchronized(this) { total = selected.size }
            for ((index, media) in selected.withIndex()) {
                ensureCurrent(id); attempting = true
                synchronized(this) { message = "Processing image ${index + 1} of ${selected.size}…" }
                val output = File(folder, "work.img")
                val previewIndex = synchronized(this) { previews.size }
                val before = File(folder, if (previewIndex < 20) "$previewIndex-before.jpg" else "work-before.jpg")
                val after = File(folder, if (previewIndex < 20) "$previewIndex-after.jpg" else "work-after.jpg")
                var keepPreviews = false
                try {
                    val started = System.nanoTime()
                    val input = environment.input(media); ensureCurrent(id)
                    val size = environment.render(input, output, before, after)
                    ensureCurrent(id)
                    if (System.nanoTime() - started > TimeUnit.MINUTES.toNanos(10)) throw ImageFailure("Image exceeded 10 minutes")
                    check(output.isFile && output.length() in 1..(64L * 1024 * 1024)) { "Invalid 4K output storage" }
                    val rendered = Output(media.id, media.name.substringBeforeLast('.') + "-4K.jpg", size.first, size.second, output, before, after, media.sourceId, multipleSources)
                    val expected = synchronized(this) { revision }
                    val publication = environment.append(id, rendered, completed, expected)
                    // Cancellation may arrive after durable publication. Record that successful
                    // append even then, preserving the source and completed count.
                    synchronized(this) {
                        revision = publication.revision; saved = JSONObject(publication.source.toString())
                        completed++; processed++
                        if (previews.size < 20) { previews += rendered.copy(mediaId = publication.mediaId, publishedSourceId = publication.sourceId); keepPreviews = true }
                    }
                } catch (e: ImageFailure) {
                    ensureCurrent(id)
                    synchronized(this) { failed++; processed++; message = "Skipped ${media.name}: ${e.message}" }
                } finally {
                    output.delete()
                    if (!keepPreviews) { before.delete(); after.delete() }
                }
                attempting = false
            }
            ensureCurrent(id)
            synchronized(this) {
                phase = if (completed > 0) "ready" else "error"
                if (completed == 0) error = "No images could be processed"
                message = "Saved $completed images${if (failed > 0) "; skipped $failed" else ""}."
            }
        } catch (e: Throwable) {
            synchronized(this) {
                if (phase != "cancelled" && !closed) {
                    if (attempting) { processed++; failed++ }
                    phase = "error"
                    error = if (e is OutOfMemoryError) "Not enough device memory for 4K processing" else e.message ?: "4K processing failed"
                }
            }
        } finally {
            // Future.cancel before execution would prevent this cleanup; cancellation instead
            // interrupts only the live thread and leaves the serial wrapper scheduled.
            Thread.interrupted()
            File(folder, "work.img").delete(); File(folder, "work-before.jpg").delete(); File(folder, "work-after.jpg").delete()
            val remove = synchronized(this) { closed || previews.isEmpty() }
            if (remove) runCatching { UpscaleFiles.removeChild(environment.directory, id) }
            synchronized(this) { workerThread = null; running = false }
        }
    }
    @Synchronized fun active(): Boolean = running || maintenance
    @Synchronized fun status(): JSONObject = JSONObject().put("jobId", job ?: JSONObject.NULL).put("phase", phase).put("mode", "fast").put("model", JSONObject.NULL)
        .put("completed", completed).put("processed", processed).put("failed", failed).put("total", total)
        .put("downloadedBytes", 0).put("totalDownloadBytes", 0).put("error", error ?: JSONObject.NULL).put("message", message ?: JSONObject.NULL)
        .put("source", saved?.let { JSONObject(it.toString()) } ?: JSONObject.NULL).put("results", JSONArray().apply {
            previews.forEachIndexed { index, output -> put(JSONObject().put("id", output.id).put("name", output.name).put("width", output.width).put("height", output.height)
                .apply { output.mediaId?.let { put("mediaId", it) }; output.publishedSourceId?.let { put("sourceId", it) } }
                .put("originalPreview", "${TrustedWebView.origin}/upscale/$job/$index-before.jpg").put("resultPreview", "${TrustedWebView.origin}/upscale/$job/$index-after.jpg")) }
        })
    fun cancel(jobId: String? = null): JSONObject {
        val cancelled = synchronized(this) {
            if (job != null && (jobId == null || jobId == job) && phase == "processing") {
                phase = "cancelled"; message = "Cancelled. Completed images are saved."; workerThread?.interrupt(); true
            } else false
        }
        if (cancelled) environment.cancelWork()
        return status()
    }
    @Synchronized fun save(jobId: String): JSONObject {
        check(job == jobId) { "4K job no longer available" }
        return saved?.let { JSONObject(it.toString()) } ?: error("No completed images have been saved")
    }
    fun storage(): JSONObject = environment.storage()
    private fun maintenance(action: () -> JSONObject): JSONObject {
        synchronized(this) { check(!closed && !running && !maintenance) { "Wait for the active 4K job to finish cleanup" }; maintenance = true }
        try { return action() } finally { synchronized(this) { maintenance = false } }
    }
    fun clearCache(): JSONObject = maintenance {
        val result = environment.clearCache()
        synchronized(this) { previews.clear() }
        result
    }
    fun deleteCollection(collectionId: String): JSONObject = maintenance {
        val result = environment.deleteCollection(collectionId)
        synchronized(this) { if (saved != null && environmentCollectionId(saved!!) == collectionId) { saved = null; previews.clear() } }
        result
    }
    private fun environmentCollectionId(source: JSONObject): String = source.optString("location").removePrefix("wallta-upscale://")
    fun preview(path: String): File? {
        val match = Regex("([a-f0-9-]{36})/(\\d{1,2})-(before|after)\\.jpg").matchEntire(path) ?: return null
        val file = synchronized(this) {
            if (match.groupValues[1] != job) return null
            val output = previews.getOrNull(match.groupValues[2].toInt()) ?: return null
            if (match.groupValues[3] == "before") output.before else output.after
        }
        return file.takeIf { it.isFile && !java.nio.file.Files.isSymbolicLink(it.toPath()) }
    }
    override fun close() {
        synchronized(this) {
            if (closed) return
            closed = true; if (running) phase = "cancelled"; workerThread?.interrupt()
            worker.shutdown()
        }
        environment.cancelWork()
        try { worker.awaitTermination(250, TimeUnit.MILLISECONDS) } catch (_: InterruptedException) { Thread.currentThread().interrupt() }
        val remove = synchronized(this) { if (running || maintenance) null else job }
        if (remove != null) runCatching { UpscaleFiles.removeChild(environment.directory, remove) }
    }
}
