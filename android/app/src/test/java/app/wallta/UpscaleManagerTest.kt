package app.wallta

import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class UpscaleManagerTest {
    private class Env : UpscaleManager.Environment {
        private val root = java.nio.file.Files.createTempDirectory("wallta-upscale-test").toFile().canonicalFile
        override val directory = File(root, "cache").apply { mkdirs() }
        val durable = File(root, "durable").apply { mkdirs() }
        @Volatile var currentRevision = 1L
        @Volatile var commits = 0
        var library = List(75) { Media(Policy.hash("image$it"), "Original.png", "content://test/$it", "source") }
        var failAt = -1; var fatalAt = -1; var blockAt = -1
        var beforePublish: (() -> Unit)? = null
        val entered = CountDownLatch(1); val release = CountDownLatch(1)
        private var renders = 0
        override fun revision() = currentRevision
        override fun resolve(ids: List<String>) = ids.map { id -> library.find { it.id == id } ?: error("Image missing") }
        override fun snapshot(sourceId: String?) = library.filter { sourceId == null || it.sourceId == sourceId }
        override fun input(media: Media) = File(directory, "original").apply { writeText("original untouched") }
        override fun render(input: File, output: File, before: File, after: File): Pair<Int, Int> {
            val index = renders++
            if (index == blockAt) { entered.countDown(); release.await(5, TimeUnit.SECONDS) }
            if (index == failAt) throw UpscaleManager.ImageFailure("Corrupt image")
            if (index == fatalAt) error("Storage unavailable")
            output.writeText("processed"); before.writeText("before"); after.writeText("after")
            return 3840 to 2160
        }
        override fun append(jobId: String, output: UpscaleManager.Output, index: Int, revision: Long): UpscaleManager.Publication {
            beforePublish?.invoke()
            check(currentRevision == revision && !Thread.currentThread().isInterrupted) { "Sources changed" }
            val folder = File(durable, jobId).apply { mkdirs() }
            output.file.copyTo(File(folder, "$index.img"))
            commits++; currentRevision++
            return UpscaleManager.Publication(JSONObject().put("id", "saved").put("name", "Ảnh 4K").put("enabled", false), currentRevision)
        }
        fun cleanup() { root.deleteRecursively() }
    }
    private fun await(manager: UpscaleManager, phase: String): JSONObject {
        val end = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (System.nanoTime() < end) {
            val status = manager.status()
            if (status.getString("phase") == phase && !manager.active()) return status
            Thread.yield()
        }
        throw AssertionError(manager.status().toString())
    }
    @Test fun allOver50PublishesOneDisabledCollectionIncrementallyAndBoundsPreviews() {
        val env = Env(); val manager = UpscaleManager(env)
        try {
            val id = manager.startAll(null, "fast").getString("jobId")
            val status = await(manager, "ready")
            assertEquals(75, status.getInt("total")); assertEquals(75, status.getInt("completed")); assertEquals(75, status.getInt("processed"))
            assertEquals(0, status.getInt("failed")); assertEquals(20, status.getJSONArray("results").length())
            assertEquals(75, env.commits); assertEquals(1, env.durable.listFiles()!!.size)
            assertEquals(75, File(env.durable, id).listFiles()!!.size)
            assertFalse(status.getJSONObject("source").getBoolean("enabled"))
            assertEquals(manager.save(id).toString(), manager.save(id).toString())
            assertEquals(75, env.commits)
            assertEquals("original untouched", File(env.directory, "original").readText())
        } finally { manager.close(); env.cleanup() }
    }
    @Test fun sourceScopeAndFastOnlyModelValidation() {
        val env = Env().apply { library = library.take(2).mapIndexed { i, m -> m.copy(sourceId = "source$i") } }
        val manager = UpscaleManager(env)
        try {
            assertThrows(IllegalArgumentException::class.java) { manager.startAll(null, "ai") }
            assertThrows(IllegalArgumentException::class.java) { manager.startAll(null, "fast", "photo") }
            assertThrows(IllegalArgumentException::class.java) { manager.start(env.library.map { it.id } + List(50) { Policy.hash("extra$it") }, "fast") }
            manager.startAll("source1", "fast")
            assertEquals(1, await(manager, "ready").getInt("completed"))
        } finally { manager.close(); env.cleanup() }
    }
    @Test fun perImageFailureSkipsAndFatalErrorRetainsDurableOutputs() {
        for (fatal in listOf(false, true)) {
            val env = Env().apply { library = library.take(3); if (fatal) fatalAt = 1 else failAt = 1 }
            val manager = UpscaleManager(env)
            try {
                val id = manager.startAll(null, "fast").getString("jobId")
                val status = await(manager, if (fatal) "error" else "ready")
                assertEquals(if (fatal) 1 else 2, status.getInt("completed"))
                assertEquals(1, status.getInt("failed")); assertEquals(if (fatal) 2 else 3, status.getInt("processed"))
                assertEquals("saved", manager.save(id).getString("id"))
                assertEquals(status.getInt("completed"), File(env.durable, id).listFiles()!!.size)
            } finally { manager.close(); env.cleanup() }
        }
    }
    @Test fun cancellationRetainsCompletedFilesAndBlocksRestartUntilWorkerExits() {
        val env = Env().apply { library = library.take(3); blockAt = 1 }; val manager = UpscaleManager(env)
        try {
            val id = manager.startAll(null, "fast").getString("jobId")
            assertTrue(env.entered.await(5, TimeUnit.SECONDS)); assertEquals(1, env.commits)
            assertNotEquals("cancelled", manager.cancel("other").getString("phase"))
            manager.cancel(id); env.release.countDown()
            val status = await(manager, "cancelled")
            assertEquals(1, status.getInt("completed")); assertEquals("saved", manager.save(id).getString("id"))
            assertEquals(1, File(env.durable, id).listFiles()!!.size)
            assertNotNull(manager.preview("$id/0-after.jpg"))
        } finally { manager.close(); env.cleanup() }
    }
    @Test fun externalRevisionAfterFirstOwnPublicationStopsQueue() {
        val env = Env().apply { library = library.take(3); blockAt = 1 }; val manager = UpscaleManager(env)
        try {
            val id = manager.startAll(null, "fast").getString("jobId")
            assertTrue(env.entered.await(5, TimeUnit.SECONDS)); env.currentRevision++; env.release.countDown()
            assertTrue(await(manager, "error").getString("error").contains("changed"))
            assertEquals(1, env.commits); assertEquals("saved", manager.save(id).getString("id"))
        } finally { manager.close(); env.cleanup() }
    }
    @Test fun publicationNeverHoldsManagerMonitorWhileEnteringUpdaterGate() {
        val env = Env().apply { library = library.take(1) }; val manager = UpscaleManager(env)
        val executor = java.util.concurrent.Executors.newSingleThreadExecutor()
        val gate = java.util.concurrent.locks.ReentrantLock()
        val enteredPublish = CountDownLatch(1); val startHasGate = CountDownLatch(1)
        env.beforePublish = {
            enteredPublish.countDown(); check(startHasGate.await(2, TimeUnit.SECONDS))
            check(gate.tryLock(500, TimeUnit.MILLISECONDS)) { "Updater/manager lock inversion" }; gate.unlock()
        }
        try {
            manager.startAll(null, "fast"); assertTrue(enteredPublish.await(2, TimeUnit.SECONDS))
            executor.submit {
                gate.lock()
                try { startHasGate.countDown(); assertThrows(IllegalStateException::class.java) { manager.startAll(null, "fast") } }
                finally { gate.unlock() }
            }.get(2, TimeUnit.SECONDS)
            assertEquals(1, await(manager, "ready").getInt("completed"))
        } finally { executor.shutdownNow(); manager.close(); env.cleanup() }
    }
    @Test fun activeQueueRejectsCacheOrCollectionMutation() {
        val env = Env().apply { blockAt = 0 }; val manager = UpscaleManager(env)
        try {
            manager.startAll(null, "fast"); assertTrue(env.entered.await(5, TimeUnit.SECONDS))
            assertThrows(IllegalStateException::class.java) { manager.clearCache() }
            assertThrows(IllegalStateException::class.java) { manager.deleteCollection("anything") }
            manager.cancel(); env.release.countDown(); await(manager, "cancelled")
        } finally { manager.close(); env.cleanup() }
    }
}
