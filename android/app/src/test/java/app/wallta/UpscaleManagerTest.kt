package app.wallta

import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class UpscaleManagerTest {
    private class Env : UpscaleManager.Environment {
        override val directory = java.nio.file.Files.createTempDirectory("wallta-upscale-test").toFile()
        var currentRevision = 1L; var commits = 0
        var block: CountDownLatch? = null
        var beforePublish: (() -> Unit)? = null
        val entered = CountDownLatch(1)
        override fun revision() = currentRevision
        override fun resolve(ids: List<String>) = ids.map { Media(it, "Original.png", "content://test/$it", "source") }
        override fun input(media: Media) = File(directory, "original").apply { writeText("original untouched") }
        override fun render(input: File, output: File, before: File, after: File): Pair<Int, Int> {
            entered.countDown(); block?.await(5, TimeUnit.SECONDS)
            output.writeText("processed"); before.writeText("before"); after.writeText("after")
            return 3840 to 2160
        }
        override fun publish(jobId: String, outputs: List<UpscaleManager.Output>, revision: Long): JSONObject {
            beforePublish?.invoke()
            check(currentRevision == revision); commits++; return JSONObject().put("id", "saved").put("name", "Ảnh 4K")
        }
    }
    private fun await(manager: UpscaleManager, phase: String): JSONObject {
        val end = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (System.nanoTime() < end) {
            val status = manager.status()
            if (status.getString("phase") == phase) return status
            Thread.yield()
        }
        throw AssertionError(manager.status().toString())
    }
    @Test fun progressesRealResultsAndIdempotentSavePreservesOriginal() {
        val env = Env(); val manager = UpscaleManager(env)
        try {
            val start = manager.start(listOf(Policy.hash("image")), "fast")
            val status = await(manager, "ready")
            assertEquals(1, status.getInt("completed"))
            assertEquals(1, status.getJSONArray("results").length())
            val result = status.getJSONArray("results").getJSONObject(0)
            assertEquals(3840, result.getInt("width"))
            assertTrue(result.getString("resultPreview").startsWith(TrustedWebView.origin))
            assertNotNull(manager.preview(result.getString("resultPreview").substringAfter("/upscale/")))
            assertNull(manager.preview("../original"))
            val id = start.getString("jobId")
            assertEquals(manager.save(id).toString(), manager.save(id).toString())
            assertEquals(1, env.commits)
            assertEquals("original untouched", File(env.directory, "original").readText())
        } finally { manager.close(); env.directory.deleteRecursively() }
    }
    @Test fun cancellationAndWrongJobCannotPublish() {
        val env = Env().apply { block = CountDownLatch(1) }; val manager = UpscaleManager(env)
        try {
            val id = manager.start(listOf(Policy.hash("image")), "fast").getString("jobId")
            assertTrue(env.entered.await(5, TimeUnit.SECONDS))
            assertThrows(IllegalStateException::class.java) { manager.start(listOf(Policy.hash("other")), "fast") }
            assertNotEquals("cancelled", manager.cancel("other").getString("phase"))
            assertEquals("cancelled", manager.cancel(id).getString("phase"))
            env.block!!.countDown()
            assertThrows(IllegalStateException::class.java) { manager.save(id) }
            assertEquals(0, env.commits)
        } finally { manager.close(); env.directory.deleteRecursively() }
    }
    @Test fun revisionChangeDiscardsWorkBeforeReady() {
        val env = Env().apply { block = CountDownLatch(1) }; val manager = UpscaleManager(env)
        try {
            val id = manager.start(listOf(Policy.hash("image")), "fast").getString("jobId")
            assertTrue(env.entered.await(5, TimeUnit.SECONDS)); env.currentRevision++; env.block!!.countDown()
            assertTrue(await(manager, "error").getString("error").contains("changed"))
            assertThrows(IllegalStateException::class.java) { manager.save(id) }
        } finally { manager.close(); env.directory.deleteRecursively() }
    }
    @Test fun saveDoesNotInvertUpdaterAndManagerLocksWithConcurrentStart() {
        val env = Env(); val manager = UpscaleManager(env)
        val executor = java.util.concurrent.Executors.newFixedThreadPool(2)
        val updaterGate = java.util.concurrent.locks.ReentrantLock()
        val enteredPublish = CountDownLatch(1); val startHasGate = CountDownLatch(1)
        env.beforePublish = {
            enteredPublish.countDown()
            check(startHasGate.await(2, TimeUnit.SECONDS))
            check(updaterGate.tryLock(500, TimeUnit.MILLISECONDS)) { "Updater/manager lock inversion" }
            updaterGate.unlock()
        }
        try {
            val id = manager.start(listOf(Policy.hash("image")), "fast").getString("jobId")
            await(manager, "ready")
            val save = executor.submit<JSONObject> { manager.save(id) }
            assertTrue(enteredPublish.await(2, TimeUnit.SECONDS))
            val start = executor.submit {
                updaterGate.lock()
                try {
                    startHasGate.countDown()
                    assertThrows(IllegalStateException::class.java) { manager.start(listOf(Policy.hash("other")), "fast") }
                } finally { updaterGate.unlock() }
            }
            start.get(2, TimeUnit.SECONDS)
            assertEquals("saved", save.get(2, TimeUnit.SECONDS).getString("id"))
            assertEquals(1, env.commits)
        } finally { executor.shutdownNow(); manager.close(); env.directory.deleteRecursively() }
    }
    @Test fun cancelReadyAndCloseRemoveTemporaryJobs() {
        val env = Env(); val manager = UpscaleManager(env)
        try {
            val id = manager.start(listOf(Policy.hash("image")), "fast").getString("jobId")
            await(manager, "ready")
            assertTrue(File(env.directory, id).isDirectory)
            manager.cancel(id)
            manager.close()
            assertFalse(File(env.directory, id).exists())
        } finally { manager.close(); env.directory.deleteRecursively() }
    }
}
