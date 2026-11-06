package app.wallta

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class GoogleConnectionPolicyTest {
    @Test fun brokerErrorsInSuccessfulHttpResponsesAreNotTokens() {
        val failure = JSONObject("""{"ok":false,"error":{"code":"unauthorized","message":"Connect Google again."}}""")
        val error = assertThrows(IllegalStateException::class.java) { GoogleConnectionPolicy.brokerData(failure) }
        assertEquals("Connect Google again.", error.message)
        assertEquals("user@example.com", GoogleConnectionPolicy.brokerData(JSONObject("""{"ok":true,"data":{"account":"user@example.com"}}""")).getString("account"))
    }
    @Test fun appsScriptPostRedirectOnlyAllowsGoogleContentGet() {
        val start = "https://script.google.com/macros/s/deployment/exec"
        assertTrue(GoogleConnectionPolicy.contentRedirect(start, "https://script.googleusercontent.com/macros/echo?user_content_key=opaque", 302))
        listOf("http://script.googleusercontent.com/x", "https://evil.example/x", "https://script.googleusercontent.com.evil.example/x", "https://user:password@script.googleusercontent.com/x").forEach {
            assertFalse(GoogleConnectionPolicy.contentRedirect(start, it, 302))
        }
        assertFalse(GoogleConnectionPolicy.contentRedirect(start, "https://script.googleusercontent.com/x", 307))
    }
    @Test fun folderQueriesAreSafeAndCoverSharedAndPersonalRoots() {
        assertEquals("trashed = false and mimeType = 'application/vnd.google-apps.folder' and 'root' in parents", GoogleConnectionPolicy.folderQuery("root", false))
        assertEquals("trashed = false and mimeType = 'application/vnd.google-apps.folder' and sharedWithMe", GoogleConnectionPolicy.folderQuery("root", true))
        assertThrows(IllegalArgumentException::class.java) { GoogleConnectionPolicy.folderQuery("x' or trashed=false", false) }
    }
    @Test fun publicConnectionNeverExposesBearerOrCapability() {
        val result = GoogleConnectionPolicy.public(JSONObject().put("phase", "ready").put("kind", "drive").put("account", "a@example.com").put("accessToken", "secret").put("connectionToken", "capability"))
        assertEquals("", result.getString("location"))
        assertEquals("drive", result.getString("sourceKind"))
        assertFalse(result.has("accessToken")); assertFalse(result.has("connectionToken"))
    }
}
