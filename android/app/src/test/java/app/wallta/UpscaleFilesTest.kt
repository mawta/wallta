package app.wallta

import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.util.UUID

class UpscaleFilesTest {
    @Test fun deletionRejectsEscapesAndSymlinksAndKeepsAdjacentFiles() {
        val root = Files.createTempDirectory("wallta-owned").toFile().canonicalFile; val outside = Files.createTempDirectory("wallta-outside").toFile().canonicalFile
        try {
            File(outside, "keep").writeText("original")
            for (id in listOf("../escape", outside.path, "holiday", "")) assertThrows(IllegalArgumentException::class.java) { UpscaleFiles.removeChild(root, id) }
            val id = UUID.randomUUID().toString(); val owned = UpscaleFiles.child(root, id, true)
            File(owned, "output.img").writeText("result")
            UpscaleFiles.removeChild(root, id); assertFalse(owned.exists()); assertEquals("original", File(outside, "keep").readText())
            val link = File(root, UUID.randomUUID().toString())
            val supported = runCatching { Files.createSymbolicLink(link.toPath(), outside.toPath()) }.isSuccess
            if (supported) assertThrows(IllegalArgumentException::class.java) { UpscaleFiles.removeChild(root, link.name) }
            assertEquals("original", File(outside, "keep").readText())
        } finally { root.deleteRecursively(); outside.deleteRecursively() }
    }
    @Test fun atomicOutputCanBeReplacedButLinksAndNestedTraversalAreRejected() {
        val root = Files.createTempDirectory("wallta-atomic").toFile().canonicalFile
        try {
            val target = File(root, "manifest.json")
            UpscaleFiles.atomicWrite(target, "first".toByteArray()); UpscaleFiles.atomicWrite(target, "second".toByteArray())
            assertEquals("second", target.readText()); assertEquals(1, root.listFiles()!!.size)
            assertThrows(IllegalArgumentException::class.java) { UpscaleFiles.atomicWrite(File(root, "../escape"), byteArrayOf()) }
        } finally { root.deleteRecursively() }
    }
}
