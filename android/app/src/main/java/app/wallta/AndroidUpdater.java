package app.wallta;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;
import androidx.core.content.FileProvider;
import java.io.File;
import java.util.Arrays;
import java.util.HashSet;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import org.json.JSONObject;

/** Platform boundary: the worker verifies APKs; only the main thread opens system UI. */
final class AndroidUpdater implements UpdateManager.Environment, AutoCloseable {
    static final int INSTALL_REQUEST = 43;
    private final Activity activity;
    private final StateStore store;
    private final SharedPreferences preferences;
    private final PackageInfo installed;
    private final UpdateManager manager;

    AndroidUpdater(Activity activity, StateStore store) {
        this.activity = activity;
        this.store = store;
        preferences = activity.getSharedPreferences("wallta-updates", Context.MODE_PRIVATE);
        try { installed = activity.getPackageManager().getPackageInfo(activity.getPackageName(), signatureFlags()); }
        catch (PackageManager.NameNotFoundException e) { throw new IllegalStateException("Không đọc được phiên bản Wallta đã cài.", e); }
        manager = new UpdateManager(this, new GithubUpdateTransport());
    }

    JSONObject status() { return manager.status(); }
    JSONObject check(boolean automatic) { return manager.check(automatic); }
    JSONObject download() { return manager.download(); }
    JSONObject cancel() { return manager.cancel(); }
    JSONObject install() { return manager.install(); }
    void installerResult(boolean success) { manager.installerResult(success); }
    public String currentVersion() { return installed.versionName; }
    public long currentCode() { return versionCode(installed); }
    public File directory() { return new File(activity.getCacheDir(), "updates"); }
    public long lastAttempt() { return preferences.getLong("lastAttempt", 0); }
    public void recordAttempt(long time) {
        if (!preferences.edit().putLong("lastAttempt", time).commit()) throw new IllegalStateException("Không lưu được thời điểm kiểm tra cập nhật.");
    }
    public boolean networkAllowed() {
        ConnectivityManager connectivity = activity.getSystemService(ConnectivityManager.class);
        if (connectivity == null) return false;
        NetworkCapabilities capabilities = connectivity.getNetworkCapabilities(connectivity.getActiveNetwork());
        return capabilities != null && capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            && (!store.state(false).optJSONObject("settings").optBoolean("wifiOnly") || capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI));
    }
    public void validateArchive(File file, UpdatePolicy.Release release) throws Exception {
        PackageManager packages = activity.getPackageManager();
        // Re-read the installed identity in case another installer changed the package.
        requireIdentity(packages.getPackageInfo(activity.getPackageName(), signatureFlags()),
            packages.getPackageArchiveInfo(file.getAbsolutePath(), signatureFlags()), release);
    }
    public void launchInstall(File file, BooleanSupplier active, Consumer<String> result) {
        activity.runOnUiThread(() -> {
            // Serialize the final active check, system UI launch and handoff callback with cancel/close.
            synchronized (manager) {
            if (!active.getAsBoolean() || activity.isFinishing() || activity.isDestroyed()) return;
            try {
                if (!activity.getPackageManager().canRequestPackageInstalls()) {
                    activity.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.getPackageName())));
                    result.accept("permissionRequired");
                    return;
                }
                Uri uri = FileProvider.getUriForFile(activity, activity.getPackageName() + ".updates", file);
                activity.startActivityForResult(installIntent(uri), INSTALL_REQUEST);
                result.accept("installing");
            } catch (Exception e) {
                android.util.Log.w("WalltaUpdater", "Could not open update installer", e);
                result.accept("Không mở được trình cài đặt trên thiết bị này. Hãy kiểm tra quyền cài ứng dụng và thử lại.");
            }
            }
        });
    }
    static void requireIdentity(PackageInfo current, PackageInfo candidate, UpdatePolicy.Release release) {
        if (current == null || candidate == null || release == null || current.packageName == null
            || !current.packageName.equals(candidate.packageName) || !release.version.equals(candidate.versionName)
            || versionCode(candidate) != release.code || versionCode(candidate) <= versionCode(current))
            throw new IllegalArgumentException("APK không đúng ứng dụng hoặc phiên bản cập nhật.");
        Signature[] local = signers(current), remote = signers(candidate);
        if (local == null || remote == null || local.length == 0 || remote.length == 0
            || !new HashSet<>(Arrays.asList(local)).equals(new HashSet<>(Arrays.asList(remote))))
            throw new IllegalArgumentException("Chứng chỉ ký APK không khớp với Wallta đã cài. Không thể cập nhật.");
    }
    private static Signature[] signers(PackageInfo info) {
        if (Build.VERSION.SDK_INT >= 28 && info.signingInfo != null) return info.signingInfo.getApkContentsSigners();
        return info.signatures;
    }
    private static long versionCode(PackageInfo info) {
        return Build.VERSION.SDK_INT >= 28 ? info.getLongVersionCode() : info.versionCode;
    }
    private static int signatureFlags() {
        return Build.VERSION.SDK_INT >= 28 ? PackageManager.GET_SIGNING_CERTIFICATES | PackageManager.GET_SIGNATURES : PackageManager.GET_SIGNATURES;
    }
    static Intent installIntent(Uri uri) {
        if (uri == null || !"content".equals(uri.getScheme())) throw new IllegalArgumentException("Cần URI APK nội bộ.");
        return new Intent(Intent.ACTION_INSTALL_PACKAGE).setDataAndType(uri, "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).putExtra(Intent.EXTRA_RETURN_RESULT, true);
    }
    public void close() { manager.close(); }
}
