package app.wallta;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.Signature;
import android.net.Uri;
import androidx.core.content.FileProvider;
import java.io.File;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = {26, 28})
public class AndroidUpdaterTest {
    public static class InstallerActivity extends Activity {
        Intent launched; int request = -1;
        @Override public void startActivity(Intent intent) { launched = intent; }
        @Override public void startActivityForResult(Intent intent, int code) { launched = intent; request = code; }
    }
    private PackageInfo info(String name, String version, int code, String signer) {
        PackageInfo info = new PackageInfo(); info.packageName = name; info.versionName = version; info.versionCode = code;
        info.signatures = new Signature[]{new Signature(signer.getBytes())}; return info;
    }
    private UpdatePolicy.Release release() {
        return new UpdatePolicy.Release("0.1.2", "", "", "", 4, 3);
    }
    @Test public void refusesWrongPackageVersionCodeAndSigningCertificate() throws Exception {
        PackageInfo current = info("app.wallta", "0.1.1", 2, "stable-key");
        AndroidUpdater.requireIdentity(current, info("app.wallta", "0.1.2", 3, "stable-key"), release());
        for (PackageInfo candidate : new PackageInfo[]{info("evil.app", "0.1.2", 3, "stable-key"), info("app.wallta", "0.1.2", 2, "stable-key"), info("app.wallta", "0.1.3", 3, "stable-key"), info("app.wallta", "0.1.2", 3, "other-key")})
            assertThrows(IllegalArgumentException.class, () -> AndroidUpdater.requireIdentity(current, candidate, release()));
    }
    @Test public void providerSharesOnlyUpdateCache() throws Exception {
        org.junit.Assume.assumeTrue("AndroidX FileProvider uses Android slash semantics", File.separatorChar == '/');
        Activity activity = Robolectric.buildActivity(Activity.class).setup().get();
        File updates = new File(activity.getCacheDir(), "updates"); updates.mkdirs();
        File apk = new File(updates, "verified.apk"); apk.createNewFile();
        Uri uri = FileProvider.getUriForFile(activity, "app.wallta.updates", apk);
        File privateFile = new File(activity.getCacheDir(), "private.txt"); privateFile.createNewFile();
        assertThrows(IllegalArgumentException.class, () -> FileProvider.getUriForFile(activity, "app.wallta.updates", privateFile));
    }
    @Test public void installerHasOnlyTemporaryReadGrant() {
        Intent install = AndroidUpdater.installIntent(Uri.parse("content://app.wallta.updates/updates/verified.apk"));
        assertEquals("application/vnd.android.package-archive", install.getType());
        assertTrue((install.getFlags() & Intent.FLAG_GRANT_READ_URI_PERMISSION) != 0);
        assertEquals(0, install.getFlags() & Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        assertEquals(Intent.ACTION_INSTALL_PACKAGE, install.getAction());
        assertTrue(install.getBooleanExtra(Intent.EXTRA_RETURN_RESULT, false));
        assertEquals("content", install.getData().getScheme());
    }
    @Test public void refusesUnparseableUnsignedAndNonUpgradeArchives() {
        PackageInfo current = info("app.wallta", "0.1.1", 2, "stable-key");
        assertThrows(IllegalArgumentException.class, () -> AndroidUpdater.requireIdentity(current, null, release()));
        PackageInfo unsigned = info("app.wallta", "0.1.2", 3, "stable-key"); unsigned.signatures = null;
        assertThrows(IllegalArgumentException.class, () -> AndroidUpdater.requireIdentity(current, unsigned, release()));
        assertThrows(IllegalArgumentException.class, () -> AndroidUpdater.requireIdentity(info("app.wallta", "0.1.2", 3, "stable-key"), info("app.wallta", "0.1.2", 3, "stable-key"), release()));
    }
    @Test public void deniedInstallPermissionOpensOnlyPackageSettingsAndRetainsRetry() {
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        Shadows.shadowOf(activity.getPackageManager()).setCanRequestPackageInstalls(false);
        AtomicReference<String> result = new AtomicReference<>();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            updater.launchInstall(new File(activity.getCacheDir(), "updates/verified.apk"), () -> true, result::set);
            assertEquals("permissionRequired", result.get());
            assertEquals(android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, activity.launched.getAction());
            assertEquals("package:app.wallta", activity.launched.getDataString());
            assertEquals(-1, activity.request);
        }
    }
    @Test public void cancelledQueuedInstallDoesNotOpenSystemUi() {
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        AtomicReference<String> result = new AtomicReference<>();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            updater.launchInstall(new File(activity.getCacheDir(), "updates/verified.apk"), () -> false, result::set);
            assertNull(activity.launched); assertNull(result.get());
        }
    }
    @Test public void archiveValidationReadsPlatformIdentityAndRejectsChangedSigner() throws Exception {
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        org.robolectric.shadows.ShadowPackageManager packages = Shadows.shadowOf(activity.getPackageManager());
        packages.installPackage(info("app.wallta", "0.1.1", 2, "stable-key"));
        File apk = new File(activity.getCacheDir(), "updates/verified.apk");
        packages.setPackageArchiveInfo(apk.getAbsolutePath(), info("app.wallta", "0.1.2", 3, "stable-key"));
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            assertEquals("0.1.1", updater.status().getString("currentVersion"));
            updater.validateArchive(apk, release());
            packages.setPackageArchiveInfo(apk.getAbsolutePath(), info("app.wallta", "0.1.2", 3, "other-key"));
            assertThrows(IllegalArgumentException.class, () -> updater.validateArchive(apk, release()));
        }
    }
    @Test public void attemptTimeSurvivesAdapterRecreation() {
        Activity activity = Robolectric.buildActivity(Activity.class).setup().get();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) { updater.recordAttempt(123456L); }
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) { assertEquals(123456L, updater.lastAttempt()); }
    }
    @Test public void grantedPermissionStartsInstallerForResultWithoutFinishingWallta() throws Exception {
        org.junit.Assume.assumeTrue("AndroidX FileProvider uses Android slash semantics", File.separatorChar == '/');
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        Shadows.shadowOf(activity.getPackageManager()).setCanRequestPackageInstalls(true);
        AtomicReference<String> result = new AtomicReference<>();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            File directory = new File(activity.getCacheDir(), "updates"); directory.mkdirs();
            File apk = new File(directory, "verified.apk"); apk.createNewFile();
            updater.launchInstall(apk, () -> true, result::set);
            assertEquals("installing", result.get()); assertEquals(AndroidUpdater.INSTALL_REQUEST, activity.request);
            assertEquals(Intent.ACTION_INSTALL_PACKAGE, activity.launched.getAction()); assertFalse(activity.isFinishing());
        }
    }
}
