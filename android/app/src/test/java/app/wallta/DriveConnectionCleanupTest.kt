package app.wallta

import android.content.Context
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.InputStream
import java.io.OutputStream
import java.security.Key
import java.security.KeyStoreSpi
import java.security.Provider
import java.security.Security
import java.security.cert.Certificate
import java.util.Date
import javax.crypto.spec.SecretKeySpec

/** The production StateStore still encrypts/writes/reads the real state file in this test. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28], manifest = Config.NONE)
class DriveConnectionCleanupTest {
    class TestKeyStore : KeyStoreSpi() {
        override fun engineGetKey(alias: String?, password: CharArray?): Key = SecretKeySpec(ByteArray(16) { 7 }, "AES")
        override fun engineGetCertificateChain(alias: String?): Array<Certificate>? = null
        override fun engineGetCertificate(alias: String?): Certificate? = null
        override fun engineGetCreationDate(alias: String?) = Date(0)
        override fun engineSetKeyEntry(alias: String?, key: Key?, password: CharArray?, chain: Array<out Certificate>?) {}
        override fun engineSetKeyEntry(alias: String?, key: ByteArray?, chain: Array<out Certificate>?) {}
        override fun engineSetCertificateEntry(alias: String?, cert: Certificate?) {}
        override fun engineDeleteEntry(alias: String?) {}
        override fun engineAliases(): java.util.Enumeration<String> = java.util.Collections.enumeration(listOf("wallta-state-v1"))
        override fun engineContainsAlias(alias: String?) = true
        override fun engineSize() = 1
        override fun engineIsKeyEntry(alias: String?) = true
        override fun engineIsCertificateEntry(alias: String?) = false
        override fun engineGetCertificateAlias(cert: Certificate?): String? = null
        override fun engineStore(stream: OutputStream?, password: CharArray?) {}
        override fun engineLoad(stream: InputStream?, password: CharArray?) {}
    }
    @Test fun savedEmptyConnectionSurvivesCleanupReopenAndReselectButRemovalRevokesIt() {
        val provider = object : Provider("WalltaTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        val server = java.net.ServerSocket(0, 10, java.net.InetAddress.getByName("127.0.0.1"))
        val revocations = java.util.concurrent.atomic.AtomicInteger()
        val serverWorker = kotlin.concurrent.thread(isDaemon = true) { while (!server.isClosed) runCatching { server.accept().use { socket ->
            val reader = socket.getInputStream().bufferedReader()
            if (reader.readLine().startsWith("DELETE /api/connection ")) revocations.incrementAndGet()
            while (!reader.readLine().isNullOrEmpty()) {}
            socket.getOutputStream().write("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".toByteArray())
        } } }
        try {
            val context: Context = RuntimeEnvironment.getApplication()
            val store = StateStore(context)
            val repository = MediaRepository(context, store)
            val id = "connected-drive"
            val state = store.state()
            state.getJSONArray("sources").put(JSONObject().put("id", id).put("name", "Drive").put("kind", "drive").put("location", "approved").put("enabled", true).put("recursive", true))
            store.put("relay:$id", JSONObject().put("phase", "ready").put("folder", "approved").put("token", "retained-capability").put("relayUrl", "http://127.0.0.1:${server.localPort}"))
            store.save(state)
            val old = store.state()
            val withoutFolder = store.state().apply { getJSONArray("sources").getJSONObject(0).put("location", "").put("enabled", false) }
            store.save(withoutFolder)
            repository.cleanup(old, store.state())
            val reopened = StateStore(context)
            assertEquals("retained-capability", reopened.session("relay", id, true)?.optString("token"))
            assertEquals("", reopened.state().getJSONArray("sources").getJSONObject(0).getString("location"))
            assertEquals(0, revocations.get())
            val reselected = reopened.state().apply { getJSONArray("sources").getJSONObject(0).put("location", "approved").put("enabled", true) }
            reopened.save(reselected)
            repository.cleanup(withoutFolder, reopened.state())
            assertEquals("retained-capability", reopened.session("relay", id, true)?.optString("token"))
            val unauthorized = reopened.state().apply { getJSONArray("sources").getJSONObject(0).put("location", "other") }
            assertThrows(IllegalArgumentException::class.java) { reopened.save(unauthorized) }
            assertEquals("approved", reopened.state().getJSONArray("sources").getJSONObject(0).getString("location"))
            val active = reopened.state()
            val removed = reopened.state().apply { put("sources", org.json.JSONArray()) }
            reopened.save(removed); repository.cleanup(active, reopened.state())
            assertNull(reopened.session("relay", id, true))
            assertEquals(1, revocations.get())
        } finally { server.close(); serverWorker.join(1000); Security.removeProvider(provider.name) }
    }
}
