package app.wallta

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.security.Provider
import java.security.Security

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28], manifest = Config.NONE)
class GoogleConnectionStateTest {
    private fun withStore(block: (StateStore) -> Unit) {
        val provider = object : Provider("WalltaGoogleTestKeyStore", 1.0, "Test Android keystore") {}
        provider.put("KeyStore.AndroidKeyStore", DriveConnectionCleanupTest.TestKeyStore::class.java.name)
        Security.insertProviderAt(provider, 1)
        try { block(StateStore(RuntimeEnvironment.getApplication())) } finally { Security.removeProvider(provider.name) }
    }
    private fun source(id: String, kind: String = "drive", location: String = "") = JSONObject().put("id", id).put("name", "Google").put("kind", kind).put("location", location).put("enabled", location.isNotBlank())

    @Test fun savedQrConnectionListsRootSharedNestedAndContinuationPages() = withStore { store ->
        store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "broker")
            .put("account", "a@example.com").put("connectionToken", "b".repeat(64)).put("brokerUrl", GoogleConnections.BROKER_URL))
        store.save(store.state().put("sources", JSONArray().put(source("d"))))
        val urls = mutableListOf<String>()
        var tokenRequests = 0
        val google = GoogleConnections(RuntimeEnvironment.getApplication(), store, brokerRequest = { _, body, _ ->
            assertEquals("token", body.getString("action")); tokenRequests++
            JSONObject().put("accessToken", "access").put("kind", "drive").put("account", "a@example.com")
                .put("expiresAt", System.currentTimeMillis() / 1000 + 3600)
        }, googleRequest = { url, headers, method, _, _ ->
            urls += url; assertEquals("Bearer access", headers["Authorization"]); assertEquals("GET", method)
            JSONObject().put("files", JSONArray().put(JSONObject().put("id", "folder1").put("name", "Folder"))).put("nextPageToken", "opaque/next+=page")
        })
        fun list(parent: String, shared: Boolean, token: Any) = google.folders("d", parent,
            GoogleConnectionPolicy.pageToken(JSONObject().put("pageToken", token)), shared)
        assertEquals("folder1", list("root", false, JSONObject.NULL).getJSONArray("folders").getJSONObject(0).getString("id"))
        list("root", true, JSONObject.NULL)
        list("folder1", false, JSONObject.NULL)
        val next = list("root", false, "opaque/next+=page")
        assertEquals("opaque/next+=page", next.getString("nextPageToken"))
        fun query(url: String) = java.net.URI(url).rawQuery.split('&').associate {
            val pair = it.split('=', limit = 2)
            pair[0] to java.net.URLDecoder.decode(pair.getOrElse(1) { "" }, "UTF-8")
        }
        assertEquals(listOf("", "", "", "opaque/next+=page"), urls.map { query(it)["pageToken"] })
        assertTrue(query(urls[0])["q"]!!.endsWith("'root' in parents"))
        assertTrue(query(urls[1])["q"]!!.endsWith("sharedWithMe"))
        assertTrue(query(urls[2])["q"]!!.endsWith("'folder1' in parents"))
        assertEquals(1, tokenRequests)
        assertEquals("a@example.com", store.session("googleConnection", "d", true)?.getString("account"))
    }

    @Test fun failedOldImportCannotCancelAReplacementImport() = withStore { store ->
        val context = RuntimeEnvironment.getApplication()
        val oldLocation = "wallta-import://${Policy.hash("p")}/11111111-1111-1111-1111-111111111111"
        val newLocation = "wallta-import://${Policy.hash("p")}/22222222-2222-2222-2222-222222222222"
        store.beginPending("import", "p", JSONObject().put("location", newLocation).put("phase", "copying"))
        PhotoImporter(context, store).cancel("p", oldLocation)
        assertEquals(newLocation, store.session("import", "p")?.getString("location"))
    }

    @Test fun selectedPhotosImportRunsOutsidePollAndRepeatedPollsDoNotDuplicateIt() = withStore { store ->
        val context = RuntimeEnvironment.getApplication()
        val tasks = mutableListOf<() -> Unit>()
        val pending = store.beginPending("googleConnection", "p", JSONObject().put("kind", "photos").put("mode", "native").put("phase", "selecting")
            .put("account", "a@example.com").put("pickerId", "picker").put("accessToken", "access").put("tokenExpires", Long.MAX_VALUE).put("expires", Long.MAX_VALUE))
        val google = GoogleConnections(context, store, cleanup = {}, googleRequest = { _, _, _, _, _ -> JSONObject().put("mediaItemsSet", true) }, importTask = { tasks += it })
        try {
            assertEquals("selecting", google.poll("p").getString("phase"))
            assertEquals(1, tasks.size)
            assertEquals("selecting", google.poll("p").getString("phase"))
            assertEquals(1, tasks.size)
            // An empty selection fails in the worker and becomes a pollable error, never a lost bridge reply.
            tasks.single()()
            assertEquals("error", google.poll("p").getString("phase"))
            assertEquals("error", google.startImport("p", pending).getString("phase"))
            assertEquals(1, tasks.size)
            assertEquals(pending.getString("nonce"), store.session("googleConnection", "p")?.getString("nonce"))
        } finally { google.cancel("p") }
    }

    @Test fun savingAccountWithoutFolderPromotesConnectionAndSupportsSeveralFoldersLater() = withStore { store ->
        val pending = store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("account", "a@example.com").put("mode", "native"))
        store.save(store.state().put("sources", JSONArray().put(source("d"))))
        assertNull(store.get("googleConnectionPending:d"))
        assertEquals(pending.getString("nonce"), store.session("googleConnection", "d", true)?.getString("nonce"))
        val next = store.state()
        next.getJSONArray("sources").getJSONObject(0).put("enabled", true).put("folders", JSONArray().put(JSONObject().put("location", "folder1")).put(JSONObject().put("location", "folder2")))
        store.save(next)
        assertEquals("a@example.com", store.session("googleConnection", "d", true)?.getString("account"))
    }
    @Test fun cancellingSavedConnectionNeverDisconnectsItAndCancellationOfReplacementPreservesOriginal() = withStore { store ->
        val context = RuntimeEnvironment.getApplication()
        val calls = mutableListOf<String>()
        val google = GoogleConnections(context, store, brokerRequest = { _, body, _ -> calls += body.getString("action"); JSONObject().put("disconnected", true) }, cleanup = { it() })
        val saved = store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "broker").put("account", "saved@example.com").put("connectionToken", "saved").put("brokerUrl", GoogleConnections.BROKER_URL))
        store.save(store.state().put("sources", JSONArray().put(source("d"))))
        google.cancel("d")
        assertTrue(calls.isEmpty())
        assertEquals(saved.getString("nonce"), store.session("googleConnection", "d", true)?.getString("nonce"))
        store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "broker").put("account", "replacement@example.com").put("deviceCode", "pending").put("connectionToken", "replacement").put("brokerUrl", GoogleConnections.BROKER_URL))
        google.cancel("d")
        assertEquals(listOf("cancel", "disconnect"), calls)
        assertEquals("saved@example.com", store.session("googleConnection", "d", true)?.getString("account"))
    }
    @Test fun photosPromotionKeepsAccountAssociatedWithExactNewImportLocation() = withStore { store ->
        val location = "wallta-import://${Policy.hash("p")}/11111111-1111-1111-1111-111111111111"
        store.beginPending("import", "p", JSONObject().put("phase", "ready").put("location", location))
        store.beginPending("googleConnection", "p", JSONObject().put("kind", "photos").put("phase", "ready").put("location", location).put("account", "a@example.com"))
        store.save(store.state().put("sources", JSONArray().put(source("p", "local", location))))
        assertEquals("a@example.com", store.session("googleConnection", "p", true)?.getString("account"))
        assertNull(store.get("googleConnectionPending:p"))
    }
    @Test fun silentRenewalKeepsAccountAndNeverRequestsInteractiveUi() = withStore { store ->
        val calls = mutableListOf<Pair<String?, Boolean>>()
        val pending = store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "native").put("account", "a@example.com"))
        store.save(store.state().put("sources", JSONArray().put(source("d"))))
        val google = GoogleConnections(RuntimeEnvironment.getApplication(), store, authorization = { _, kind, account, interactive ->
            assertEquals("drive", kind); calls += account to interactive; GoogleGrant("a@example.com", "renewed-token")
        })
        assertEquals("renewed-token", google.token("d"))
        assertEquals(listOf("a@example.com" to false), calls)
        assertEquals(pending.getString("nonce"), store.session("googleConnection", "d", true)?.getString("nonce"))
    }
    @Test fun brokerPairPollIsThrottledAndUnsavedDeliveredCapabilityIsDisconnected() = withStore { store ->
        val actions = mutableListOf<String>()
        val cap = "b".repeat(64); val code = "a".repeat(64)
        val google = GoogleConnections(RuntimeEnvironment.getApplication(), store, brokerRequest = { _, body, _ ->
            actions += body.getString("action")
            when (body.getString("action")) {
                "pair" -> JSONObject().put("deviceCode", code).put("verificationUrl", GoogleConnections.BROKER_URL + "?action=connect&code=" + code)
                "poll" -> JSONObject().put("phase", "ready").put("kind", "drive").put("account", "a@example.com").put("connectionToken", cap)
                else -> JSONObject()
            }
        }, cleanup = { it() })
        google.begin(JSONObject().put("sourceId", "d").put("kind", "drive").put("mode", "broker"))
        assertEquals("authorizing", google.poll("d").getString("phase")); assertEquals(listOf("pair"), actions)
        val pending = store.session("googleConnection", "d")!!; pending.put("nextPoll", 0); store.saveSession("googleConnection", "d", pending)
        val ready = google.poll("d"); assertEquals("ready", ready.getString("phase")); assertEquals("", ready.getString("location")); assertFalse(ready.has("connectionToken"))
        google.cancel("d")
        assertEquals(listOf("pair", "poll", "cancel", "disconnect"), actions); assertNull(store.session("googleConnection", "d"))
    }
    @Test fun cancelledGoogleImportCannotDownloadOrReplacePreviouslySavedPhotos() = withStore { store ->
        val context = RuntimeEnvironment.getApplication()
        val location = "wallta-import://${Policy.hash("p")}/11111111-1111-1111-1111-111111111111"
        store.put("import:p", JSONObject().put("location", location).put("phase", "ready"))
        var downloads = 0
        val item = JSONObject().put("id", "photo").put("mediaFile", JSONObject().put("mimeType", "image/jpeg").put("baseUrl", "https://lh3.googleusercontent.com/photo"))
        assertThrows(IllegalStateException::class.java) {
            PhotoImporter(context, store).importGoogle("p", listOf(item), "a@example.com", "cancelled-nonce") { downloads++; byteArrayOf() }
        }
        assertEquals(0, downloads); assertEquals(location, store.session("import", "p", true)?.getString("location"))
    }
    @Test fun savingWhileSilentTokenRenewalFinishesKeepsTheRenewedGrant() = withStore { store ->
        store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "native").put("account", "a@example.com"))
        val google = GoogleConnections(RuntimeEnvironment.getApplication(), store, authorization = { _, _, _, _ ->
            store.save(store.state().put("sources", JSONArray().put(source("d"))))
            GoogleGrant("a@example.com", "renewed-after-save")
        })
        assertEquals("renewed-after-save", google.token("d", false))
        assertEquals("renewed-after-save", store.session("googleConnection", "d", true)?.getString("accessToken"))
    }
    @Test fun reconnectingNativeDriveCanReplaceLegacyRelayWithoutLegacyFolderRestriction() = withStore { store ->
        store.put("relay:d", JSONObject().put("phase", "ready").put("folder", "legacy-folder").put("token", "cap").put("relayUrl", "http://127.0.0.1:1"))
        store.beginPending("googleConnection", "d", JSONObject().put("kind", "drive").put("phase", "ready").put("mode", "native").put("account", "a@example.com"))
        store.save(store.state().put("sources", JSONArray().put(source("d", "drive", "another-folder"))))
        assertEquals("another-folder", store.state().getJSONArray("sources").getJSONObject(0).getString("location"))
        val old = store.state()
        val next = store.state().apply { getJSONArray("sources").getJSONObject(0).put("location", "third-folder") }
        store.save(next); MediaRepository(RuntimeEnvironment.getApplication(), store).cleanup(old, store.state())
        assertEquals("a@example.com", store.session("googleConnection", "d", true)?.getString("account"))
        val current = store.state(); store.save(store.state().put("sources", JSONArray()))
        MediaRepository(RuntimeEnvironment.getApplication(), store).cleanup(current, store.state())
        assertNull(store.session("googleConnection", "d", true))
    }
}
