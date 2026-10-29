package app.wallta

import java.net.ServerSocket
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NetRedirectTest {
    @Test fun webdavCredentialsStayInsideSelectedRootsOnEveryRedirect() {
        val server = ServerSocket(0, 10, java.net.InetAddress.getByName("127.0.0.1"))
        val received = java.util.concurrent.CopyOnWriteArrayList<Pair<String, String?>>()
        val base = "http://127.0.0.1:${server.localPort}"
        val worker = kotlin.concurrent.thread(isDaemon = true) { while (!server.isClosed) runCatching { server.accept().use { socket ->
            val reader = socket.getInputStream().bufferedReader()
            val path = reader.readLine().split(' ')[1]
            val headers = mutableMapOf<String, String>()
            while (true) { val line = reader.readLine() ?: break; if (line.isEmpty()) break; headers[line.substringBefore(':').lowercase()] = line.substringAfter(':').trim() }
            received += path to headers["authorization"]
            val redirect = when (path) {
                "/a/outside" -> "$base/private/collector"
                "/a/cross" -> "http://localhost:${server.localPort}/b/image"
                "/a/selected" -> "$base/b/image"
                else -> null
            }
            val response = if (redirect != null) "HTTP/1.1 302 Found\r\nLocation: $redirect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" else "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            socket.getOutputStream().write(response.toByteArray(Charsets.US_ASCII))
        } } }
        try {
            val source = JSONObject().put("kind", "webdav").put("location", "$base/a").put("folders", JSONArray().put(JSONObject().put("location", "$base/a")).put(JSONObject().put("location", "$base/b")))
            val scope: (String) -> Boolean = { SourceFolders.contains(source, it) }
            listOf("outside", "cross").forEach { path ->
                assertThrows(IllegalArgumentException::class.java) { Net.bytes("$base/a/$path", 1024, mapOf("Authorization" to "Basic private"), allowedUrl = scope) }
            }
            assertEquals("ok", String(Net.bytes("$base/a/selected", 1024, mapOf("Authorization" to "Basic private"), allowedUrl = scope)))
            assertFalse(received.any { it.first == "/private/collector" })
            assertEquals(1, received.count { it.first == "/b/image" })
            assertEquals("Basic private", received.last().second)
        } finally { server.close(); worker.join(1000) }
    }
}
