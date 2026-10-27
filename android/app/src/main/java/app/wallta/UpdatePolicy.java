package app.wallta;

import java.math.BigInteger;
import java.net.URI;
import java.time.Instant;
import java.util.List;
import org.json.JSONObject;

/** Validation independent of Android so malformed release metadata is rejected before IO. */
final class UpdatePolicy {
    static final String MANIFEST_URL = "https://github.com/mawta/wallta/releases/latest/download/latest.json";
    static final int MANIFEST_LIMIT = 256 * 1024;
    static final long APK_LIMIT = 256L * 1024 * 1024;
    static final long CHECK_INTERVAL = 24L * 60 * 60 * 1000;
    private static final String PREFIX = "/mawta/wallta/releases/download/";
    private static final List<String> DOWNLOAD_HOSTS = java.util.Arrays.asList("release-assets.githubusercontent.com", "objects.githubusercontent.com", "github-releases.githubusercontent.com");

    static final class Release {
        final String version, notes, url, sha256;
        final long size, code;
        Release(String version, String notes, String url, String sha256, long size, long code) {
            this.version = version; this.notes = notes; this.url = url; this.sha256 = sha256; this.size = size; this.code = code;
        }
    }
    static Release release(JSONObject json, String installed, long code) {
        try {
            String version = json.getString("version");
            BigInteger[] remote = semver(version), local = semver(installed);
            String notes = json.has("notes") ? json.getString("notes") : "";
            require(notes.length() <= 65536);
            Instant.parse(json.getString("pub_date"));
            JSONObject android = json.getJSONObject("android");
            String url = android.getString("url"), hash = android.getString("sha256");
            URI uri = secureUri(url);
            require(uri != null && "github.com".equals(uri.getHost()) && uri.getRawQuery() == null);
            require((PREFIX + "v" + version + "/Wallta_" + version + "_android.apk").equals(uri.getRawPath()));
            require(hash.matches("[0-9a-f]{64}"));
            long size = integer(android.get("size")), remoteCode = integer(android.get("versionCode"));
            require(size > 0 && size <= APK_LIMIT && remoteCode > 0 && remoteCode <= Integer.MAX_VALUE);
            int comparison = 0;
            for (int i = 0; i < 3 && comparison == 0; i++) comparison = remote[i].compareTo(local[i]);
            return comparison > 0 && remoteCode > code ? new Release(version, notes, url, hash, size, remoteCode) : null;
        } catch (Exception e) { throw new IllegalArgumentException("Thông tin bản cập nhật không hợp lệ.", e); }
    }
    private static long integer(Object value) {
        require(value instanceof Integer || value instanceof Long);
        return ((Number) value).longValue();
    }
    private static BigInteger[] semver(String version) {
        require(version.length() <= 128 && version.matches("(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)"));
        String[] parts = version.split("\\.");
        return new BigInteger[]{new BigInteger(parts[0]), new BigInteger(parts[1]), new BigInteger(parts[2])};
    }
    static boolean redirectAllowed(String url) {
        URI uri = secureUri(url);
        if (uri == null) return false;
        if (DOWNLOAD_HOSTS.contains(uri.getHost())) return true;
        return "github.com".equals(uri.getHost()) && uri.getRawQuery() == null
            && (MANIFEST_URL.equals(url) || uri.getRawPath().matches("/mawta/wallta/releases/download/v[0-9]+\\.[0-9]+\\.[0-9]+/(latest\\.json|Wallta_[0-9]+\\.[0-9]+\\.[0-9]+_android\\.apk)"));
    }
    private static URI secureUri(String url) {
        try {
            URI uri = new URI(url);
            if (!"https".equals(uri.getScheme()) || uri.getHost() == null || uri.getRawUserInfo() != null || uri.getRawFragment() != null
                || (uri.getPort() != -1 && uri.getPort() != 443) || uri.getRawPath() == null || uri.getRawPath().contains("%")
                || uri.getRawPath().contains("\\") || !uri.normalize().getRawPath().equals(uri.getRawPath())) return null;
            return uri;
        } catch (Exception e) { return null; }
    }
    static boolean checkAllowed(boolean automatic, long now, long last) {
        return !automatic || last == 0 || (now >= last && now - last >= CHECK_INTERVAL);
    }
    private static void require(boolean condition) { if (!condition) throw new IllegalArgumentException("Invalid release metadata"); }
}
