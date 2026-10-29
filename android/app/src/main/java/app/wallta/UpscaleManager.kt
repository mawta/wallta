package app.wallta

import java.io.File
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.Future
import org.json.JSONArray
import org.json.JSONObject

class UpscaleManager(private val environment: Environment) : AutoCloseable {
    data class Output(val id: String, val name: String, val width: Int, val height: Int, val file: File, val before: File, val after: File)
    interface Environment {
        val directory: File
        fun revision(): Long
        fun resolve(ids: List<String>): List<Media>
        fun input(media: Media): File
        fun render(input: File, output: File, before: File, after: File): Pair<Int, Int>
        fun publish(jobId: String, outputs: List<Output>, revision: Long): JSONObject
    }
    private val worker = Executors.newSingleThreadExecutor()
    private var task: Future<*>? = null
    private var closed = false
    private var job: String? = null
    private var phase = "idle"
    private var revision = 0L
    private var total = 0
    private var error: String? = null
    private var deadline = 0L
    private val outputs = mutableListOf<Output>()
    private var saved: JSONObject? = null
    private var saving = false
    @Synchronized fun start(ids: List<String>, mode: String): JSONObject {
        UpscalePolicy.selection(ids, mode)
        check(!closed && phase != "processing" && !saving) { "A 4K job is already running or being saved" }
        val id = UUID.randomUUID().toString()
        job = id; phase = "processing"; revision = environment.revision(); total = ids.size; error = null; outputs.clear(); saved = null
        deadline = System.nanoTime() + java.util.concurrent.TimeUnit.MINUTES.toNanos(10)
        val expected = revision
        task = worker.submit {
            val folder = File(environment.directory, id)
            try {
                environment.directory.listFiles()?.filter { it.isDirectory && it.name != id && it.name.matches(Regex("[a-f0-9-]{36}")) }?.forEach { it.deleteRecursively() }
                check(folder.mkdirs()) { "Cannot create 4K working storage" }
                val selected = environment.resolve(ids)
                check(selected.map { it.id } == ids) { "Selected images are no longer available" }
                var bytes = 0L
                selected.forEachIndexed { index, media ->
                    ensureCurrent(id, expected)
                    val input = environment.input(media)
                    ensureCurrent(id, expected)
                    val output = File(folder, "$index.img"); val before = File(folder, "$index-before.jpg"); val after = File(folder, "$index-after.jpg")
                    val dimensions = environment.render(input, output, before, after)
                    bytes += output.length() + before.length() + after.length()
                    require(bytes <= 512L * 1024 * 1024) { "4K results exceed 512 MB. Choose fewer images." }
                    synchronized(this) {
                        ensureCurrent(id, expected)
                        outputs += Output(media.id, media.name.substringBeforeLast('.') + "-4K.jpg", dimensions.first, dimensions.second, output, before, after)
                    }
                }
                synchronized(this) { ensureCurrent(id, expected); phase = "ready" }
            } catch (e: Throwable) {
                synchronized(this) { if (job == id && phase != "cancelled" && !closed) { phase = "error"; error = if (e is OutOfMemoryError) "Not enough device memory for 4K processing. Close other apps or choose a smaller image." else e.message ?: "4K processing failed"; outputs.clear() } }
                folder.deleteRecursively()
            }
        }
        return status()
    }
    @Synchronized private fun ensureCurrent(id: String, expected: Long) {
        check(!closed && job == id && phase == "processing" && !Thread.currentThread().isInterrupted) { "4K processing cancelled" }
        check(environment.revision() == expected) { "Sources changed. Start 4K processing again." }
        check(System.nanoTime() < deadline) { "4K job exceeded 10 minutes. Choose fewer images." }
    }
    @Synchronized fun status(): JSONObject = JSONObject().put("jobId", job ?: JSONObject.NULL).put("phase", phase).put("mode", "fast")
        .put("completed", outputs.size).put("total", total).put("error", error ?: JSONObject.NULL).put("results", JSONArray().apply {
            outputs.forEachIndexed { index, output -> put(JSONObject().put("id", output.id).put("name", output.name).put("width", output.width).put("height", output.height)
                .put("originalPreview", "${TrustedWebView.origin}/upscale/$job/$index-before.jpg").put("resultPreview", "${TrustedWebView.origin}/upscale/$job/$index-after.jpg")) }
        })
    @Synchronized fun cancel(jobId: String? = null): JSONObject {
        if (!saving && job != null && (jobId == null || jobId == job) && phase in listOf("processing", "ready")) {
            phase = "cancelled"; outputs.clear(); task?.cancel(true)
            val cancelled = job!!
            worker.submit { File(environment.directory, cancelled).deleteRecursively() }
        }
        return status()
    }
    fun save(jobId: String): JSONObject {
        val snapshot = synchronized(this) {
            check(!closed && job == jobId) { "4K job no longer available" }
            saved?.let { return JSONObject(it.toString()) }
            check(!saving && phase == "ready" && outputs.isNotEmpty() && environment.revision() == revision) { "4K results are not ready, are being saved, or sources changed" }
            saving = true
            outputs.toList() to revision
        }
        // Publication enters the updater/state gates. Never hold this monitor while acquiring them.
        try {
            val source = environment.publish(jobId, snapshot.first, snapshot.second)
            synchronized(this) { saved = JSONObject(source.toString()) }
            return JSONObject(source.toString())
        } finally {
            val cleanup = synchronized(this) { saving = false; closed }
            if (cleanup) File(environment.directory, jobId).deleteRecursively()
        }
    }
    @Synchronized fun preview(path: String): File? {
        val match = Regex("([a-f0-9-]{36})/(\\d{1,2})-(before|after)\\.jpg").matchEntire(path) ?: return null
        if (match.groupValues[1] != job || phase !in listOf("processing", "ready")) return null
        val output = outputs.getOrNull(match.groupValues[2].toInt()) ?: return null
        return (if (match.groupValues[3] == "before") output.before else output.after).takeIf { it.isFile }
    }
    override fun close() {
        synchronized(this) {
            if (closed) return
            closed = true; task?.cancel(true); outputs.clear()
            if (!saving) worker.submit { environment.directory.listFiles()?.filter { it.isDirectory && it.name.matches(Regex("[a-f0-9-]{36}")) }?.forEach { it.deleteRecursively() } }
            worker.shutdown()
        }
        try { worker.awaitTermination(250, java.util.concurrent.TimeUnit.MILLISECONDS) } catch (_: InterruptedException) { Thread.currentThread().interrupt() }
    }
}
