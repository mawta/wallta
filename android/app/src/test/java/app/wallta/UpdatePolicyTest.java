package app.wallta;

import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpdatePolicyTest {
    private JSONObject manifest() throws Exception {
        return new JSONObject().put("version", "0.1.2").put("notes", "Fixes").put("pub_date", "2026-10-27T00:00:00Z")
            .put("android", new JSONObject().put("url", "https://github.com/mawta/wallta/releases/download/v0.1.2/Wallta_0.1.2_android.apk")
                .put("sha256", "a".repeat(64)).put("size", 1234).put("versionCode", 3));
    }
    @Test public void acceptsOnlyHigherStableVersionAndCode() throws Exception {
        assertNotNull(UpdatePolicy.release(manifest(), "0.1.1", 2));
        assertNull(UpdatePolicy.release(manifest(), "0.1.2", 2));
        assertNull(UpdatePolicy.release(manifest(), "0.1.1", 3));
        for (String version : new String[]{"0.1.2-beta", "v0.1.2", "00.1.2", "0.1", "0.1.2+build"}) {
            assertThrows(IllegalArgumentException.class, () -> UpdatePolicy.release(manifest().put("version", version), "0.1.1", 2));
        }
    }
    @Test public void rejectsUntrustedAssetUrlsAndMalformedMetadata() throws Exception {
        for (String url : new String[]{"http://github.com/mawta/wallta/releases/download/v0.1.2/Wallta_0.1.2_android.apk", "https://github.com/other/wallta/releases/download/v0.1.2/x.apk", "https://github.com@evil.test/mawta/wallta/releases/download/v0.1.2/x.apk", "https://github.com/mawta/wallta/releases/download/v0.1.2/%2e%2e/x.apk", "https://github.com/mawta/wallta/releases/download/v0.1.2/Wallta_0.1.2_android.apk?x=1"}) {
            JSONObject json = manifest(); json.getJSONObject("android").put("url", url);
            assertThrows(IllegalArgumentException.class, () -> UpdatePolicy.release(json, "0.1.1", 2));
        }
        for (Object size : new Object[]{0, -1, UpdatePolicy.APK_LIMIT + 1, 1.5, "1234"}) {
            JSONObject json = manifest(); json.getJSONObject("android").put("size", size);
            assertThrows(IllegalArgumentException.class, () -> UpdatePolicy.release(json, "0.1.1", 2));
        }
        JSONObject json = manifest(); json.getJSONObject("android").put("sha256", "A".repeat(64));
        assertThrows(IllegalArgumentException.class, () -> UpdatePolicy.release(json, "0.1.1", 2));
    }
    @Test public void limitsRedirectsToExactHttpsDownloadHosts() {
        assertTrue(UpdatePolicy.redirectAllowed("https://release-assets.githubusercontent.com/github-production-release-asset/123/file?sig=abc"));
        for (String url : new String[]{"http://release-assets.githubusercontent.com/a", "https://evil.githubusercontent.com/a", "https://release-assets.githubusercontent.com.evil.test/a", "https://u:p@github.com/a", "https://github.com:444/a", "https://github.com/a#x", "https://github.com/%2e%2e/a", "https://github.com/other/project/releases/download/v1/x"}) assertFalse(url, UpdatePolicy.redirectAllowed(url));
    }
    @Test public void throttleBlocksClockRollbackAndAllowsManualChecks() {
        assertTrue(UpdatePolicy.checkAllowed(false, 100, 100));
        assertFalse(UpdatePolicy.checkAllowed(true, 100, 100));
        assertFalse(UpdatePolicy.checkAllowed(true, 50, 100));
        assertTrue(UpdatePolicy.checkAllowed(true, 100 + UpdatePolicy.CHECK_INTERVAL, 100));
        assertTrue(UpdatePolicy.checkAllowed(true, 100, 0));
    }
}
