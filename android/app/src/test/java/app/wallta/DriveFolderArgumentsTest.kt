package app.wallta

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26, 28], manifest = Config.NONE)
class DriveFolderArgumentsTest {
    @Test fun nullableFirstPageFromFrontendDoesNotBecomeLiteralNull() {
        val request = JSONObject("""{"sourceId":"drive","parentId":"root","pageToken":null,"shared":false}""")
        assertEquals("", GoogleConnectionPolicy.pageToken(request))
        assertEquals("", GoogleConnectionPolicy.pageToken(JSONObject()))
    }
    @Test fun continuationTokenIsPreservedExactly() {
        val token = "opaque/next+=page"
        assertEquals(token, GoogleConnectionPolicy.pageToken(JSONObject().put("pageToken", token)))
    }
}
