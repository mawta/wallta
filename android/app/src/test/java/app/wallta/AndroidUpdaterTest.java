package app.wallta;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
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
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = {26, 28})
public class AndroidUpdaterTest {
    public static class InstallerActivity extends Activity {
        Intent launched; int request = -1;
        Runnable beforePackageQuery, beforeLaunch;
        @Override public PackageManager getPackageManager() {
            PackageManager packages = super.getPackageManager();
            Runnable hook = beforePackageQuery; beforePackageQuery = null;
            if (hook != null) hook.run();
            return packages;
        }
        @Override public void startActivity(Intent intent) { if (beforeLaunch != null) beforeLaunch.run(); launched = intent; }
        @Override public void startActivityForResult(Intent intent, int code) { launched = intent; request = code; }
    }
    private PackageInfo info(String name, String version, int code, String signer) {
        PackageInfo info = new PackageInfo(); info.packageName = name; info.versionName = version; info.versionCode = code;
        info.signatures = new Signature[]{new Signature(signer.getBytes())}; return info;
    }
    private UpdatePolicy.Release release() {
        return new UpdatePolicy.Release("0.1.2", "", "", "", 4, 3);
    }
    private void attachUpdateProvider(Activity activity) {
        android.content.pm.ProviderInfo provider = activity.getPackageManager().resolveContentProvider(
            activity.getPackageName() + ".updates", PackageManager.GET_META_DATA);
        assertNotNull("Update provider must be declared in the app manifest", provider);
        // Each Robolectric test has a new dataDir; attachment clears FileProvider's static roots cache.
        new FileProvider().attachInfo(activity, provider);
    }
    @Test public void refusesWrongPackageVersionCodeAndSigningCertificate() throws Exception {
        PackageInfo current = info("app.wallta", "0.1.1", 2, "stable-key");
        AndroidUpdater.requireIdentity(current, info("app.wallta", "0.1.2", 3, "stable-key"), release());
        for (PackageInfo candidate : new PackageInfo[]{info("evil.app", "0.1.2", 3, "stable-key"), info("app.wallta", "0.1.2", 2, "stable-key"), info("app.wallta", "0.1.3", 3, "stable-key"), info("app.wallta", "0.1.2", 3, "other-key")})
            assertThrows(IllegalArgumentException.class, () -> AndroidUpdater.requireIdentity(current, candidate, release()));
    }
    @Test public void providerSharesOnlyUpdateCache() throws Exception {
        Activity activity = Robolectric.buildActivity(Activity.class).setup().get();
        attachUpdateProvider(activity);
        org.junit.Assume.assumeTrue("AndroidX FileProvider uses Android slash semantics", File.separatorChar == '/');
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
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        attachUpdateProvider(activity);
        org.junit.Assume.assumeTrue("AndroidX FileProvider uses Android slash semantics", File.separatorChar == '/');
        Shadows.shadowOf(activity.getPackageManager()).setCanRequestPackageInstalls(true);
        AtomicReference<String> result = new AtomicReference<>();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            File directory = new File(activity.getCacheDir(), "updates"); directory.mkdirs();
            File apk = new File(directory, "verified.apk"); apk.createNewFile();
            assertEquals("content", FileProvider.getUriForFile(activity, "app.wallta.updates", apk).getScheme());
            updater.launchInstall(apk, () -> true, result::set);
            assertEquals("installing", result.get()); assertEquals(AndroidUpdater.INSTALL_REQUEST, activity.request);
            assertEquals(Intent.ACTION_INSTALL_PACKAGE, activity.launched.getAction()); assertFalse(activity.isFinishing());
        }
    }
    @Test public void cancellationCannotInterleaveWithSystemUiHandoff() throws Exception { assertAtomicHandoff(false); }
    @Test public void closeCannotInterleaveWithSystemUiHandoff() throws Exception { assertAtomicHandoff(true); }
    private void assertAtomicHandoff(boolean close) throws Exception {
        InstallerActivity activity = Robolectric.buildActivity(InstallerActivity.class).setup().get();
        Shadows.shadowOf(activity.getPackageManager()).setCanRequestPackageInstalls(false);
        CountDownLatch atPermissionQuery = new CountDownLatch(1), releasePermissionQuery = new CountDownLatch(1);
        CountDownLatch invalidationStarted = new CountDownLatch(1), invalidationFinished = new CountDownLatch(1);
        AtomicBoolean blockedOnHandoff = new AtomicBoolean();
        AtomicInteger sequence = new AtomicInteger(), launchOrder = new AtomicInteger(), callbackOrder = new AtomicInteger(), invalidateOrder = new AtomicInteger();
        AtomicReference<Throwable> failure = new AtomicReference<>();
        try (AndroidUpdater updater = new AndroidUpdater(activity, new StateStore(activity))) {
            java.lang.reflect.Field managerField = AndroidUpdater.class.getDeclaredField("manager"); managerField.setAccessible(true);
            UpdateManager manager = (UpdateManager) managerField.get(updater);
            java.lang.reflect.Field workerField = UpdateManager.class.getDeclaredField("worker"); workerField.setAccessible(true);
            ((java.util.concurrent.ExecutorService) workerField.get(manager)).submit(() -> {}).get(5, TimeUnit.SECONDS);
            // Model the real verified worker-to-main handoff without a network request or real APK.
            java.lang.reflect.Field busy = UpdateManager.class.getDeclaredField("busy"); busy.setAccessible(true); busy.setBoolean(manager, true);
            java.lang.reflect.Field phase = UpdateManager.class.getDeclaredField("phase"); phase.setAccessible(true); phase.set(manager, "installing");
            activity.beforePackageQuery = () -> {
                atPermissionQuery.countDown();
                try { if (!releasePermissionQuery.await(5, TimeUnit.SECONDS)) throw new AssertionError("Timed out releasing permission lookup"); }
                catch (InterruptedException e) { throw new AssertionError(e); }
            };
            activity.beforeLaunch = () -> launchOrder.set(sequence.incrementAndGet());
            Thread invalidator = new Thread(() -> {
                try {
                    if (!atPermissionQuery.await(5, TimeUnit.SECONDS)) throw new AssertionError("Permission lookup never started");
                    invalidationStarted.countDown();
                    if (close) updater.close(); else updater.cancel();
                    invalidateOrder.set(sequence.incrementAndGet());
                } catch (Throwable e) { failure.set(e); }
                finally { invalidationFinished.countDown(); }
            }, "update-invalidator");
            Thread releaser = new Thread(() -> {
                try {
                    if (!invalidationStarted.await(5, TimeUnit.SECONDS)) throw new AssertionError("Invalidation never started");
                    long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
                    while (invalidator.getState() != Thread.State.BLOCKED && invalidationFinished.getCount() != 0 && System.nanoTime() < deadline) Thread.yield();
                    blockedOnHandoff.set(invalidator.getState() == Thread.State.BLOCKED);
                } catch (Throwable e) { failure.set(e); }
                finally { releasePermissionQuery.countDown(); }
            }, "update-handoff-release");
            invalidator.start(); releaser.start();
            updater.launchInstall(new File(activity.getCacheDir(), "updates/verified.apk"), () -> true,
                result -> callbackOrder.set(sequence.incrementAndGet()));
            invalidator.join(5000); releaser.join(5000);
            assertNull(failure.get()); assertFalse(invalidator.isAlive()); assertFalse(releaser.isAlive());
            assertTrue("Cancellation/close must wait for the active system UI handoff", blockedOnHandoff.get());
            assertTrue(launchOrder.get() > 0 && launchOrder.get() < invalidateOrder.get());
            assertTrue(callbackOrder.get() > launchOrder.get() && callbackOrder.get() < invalidateOrder.get());
        }
    }
}
