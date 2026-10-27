package app.wallta;

import java.io.*;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import org.json.JSONObject;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpdateManagerTest {
    private static final byte[] APK = "a signed APK fixture validated by the platform adapter".getBytes();
    static String hash(byte[] bytes) throws Exception {
        StringBuilder result = new StringBuilder();
        for (byte b : MessageDigest.getInstance("SHA-256").digest(bytes)) result.append(String.format("%02x", b));
        return result.toString();
    }
    static class Env implements UpdateManager.Environment {
        final File directory; long last; boolean wifi = true, permission = true, valid = true, deferInstall; volatile Runnable pendingInstall;
        final AtomicInteger installs = new AtomicInteger(), validations = new AtomicInteger();
        Env() throws Exception { directory = Files.createTempDirectory("wallta-updater-").toFile(); }
        public String currentVersion() { return "0.1.1"; }
        public long currentCode() { return 2; }
        public File directory() { return directory; }
        public long lastAttempt() { return last; }
        public void recordAttempt(long time) { last = time; }
        public boolean networkAllowed() { return wifi; }
        public void validateArchive(File file, UpdatePolicy.Release release) throws Exception { validations.incrementAndGet(); if (!valid) throw new IOException("Sai chứng chỉ APK."); }
        public void launchInstall(File file, java.util.function.BooleanSupplier active, Consumer<String> result) { Runnable launch = () -> { if (active.getAsBoolean()) { installs.incrementAndGet(); result.accept(permission ? "installing" : "permissionRequired"); } }; if (deferInstall) pendingInstall = launch; else launch.run(); }
    }
    static class Transport implements UpdateManager.Transport {
        final AtomicInteger requests = new AtomicInteger(); byte[] apk = APK; String sha; long size; boolean unknownLength;
        CountDownLatch entered, unblock;
        Transport() throws Exception { sha = hash(APK); size = APK.length; }
        public UpdateManager.Response open(String url) throws Exception {
            requests.incrementAndGet();
            if (entered != null) { entered.countDown(); unblock.await(); }
            byte[] bytes = url.equals(UpdatePolicy.MANIFEST_URL) ? new JSONObject().put("version", "0.1.2").put("notes", "Fixes").put("pub_date", "2026-10-27T00:00:00Z")
                .put("android", new JSONObject().put("url", "https://github.com/mawta/wallta/releases/download/v0.1.2/Wallta_0.1.2_android.apk").put("sha256", sha).put("size", size).put("versionCode", 3)).toString().getBytes() : apk;
            return new UpdateManager.Response(new ByteArrayInputStream(bytes), unknownLength ? -1 : bytes.length, () -> {});
        }
        public void cancel() {}
    }
    private static JSONObject await(UpdateManager manager, String phase) throws Exception {
        long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(3);
        while (System.nanoTime() < end) { JSONObject status = manager.status(); if (status.getString("phase").equals(phase)) return status; Thread.sleep(5); }
        fail("Expected " + phase + ", got " + manager.status()); return null;
    }
    @Test public void checkIsAsynchronousAndDoesNotDownloadUntilUserClick() throws Exception {
        Env env = new Env(); Transport net = new Transport(); net.entered = new CountDownLatch(1); net.unblock = new CountDownLatch(1);
        try (UpdateManager manager = new UpdateManager(env, net)) {
            assertEquals("checking", manager.check(false).getString("phase"));
            assertTrue(net.entered.await(1, TimeUnit.SECONDS));
            assertEquals("checking", manager.check(false).getString("phase"));
            assertEquals(1, net.requests.get()); net.unblock.countDown();
            JSONObject status = await(manager, "available"); assertEquals("0.1.1", status.getString("currentVersion")); assertEquals("0.1.2", status.getString("version"));
            assertEquals(1, net.requests.get()); assertEquals(0, env.installs.get());
            manager.download(); await(manager, "ready"); assertEquals(2, net.requests.get());
            assertEquals(APK.length, manager.status().getLong("downloadedBytes"));
        }
    }
    @Test public void hashMismatchNeverReachesReadyAndDeletesPartial() throws Exception {
        Env env = new Env(); Transport net = new Transport(); net.sha = "0".repeat(64);
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); await(manager, "available"); manager.download();
            assertTrue(await(manager, "error").getString("error").contains("SHA-256"));
            assertEquals(0, env.validations.get()); assertEquals(0, env.installs.get());
        }
        assertEquals(0, env.directory.list().length);
    }
    @Test public void declaredSizeIsEnforcedWhileStreaming() throws Exception {
        Env env = new Env(); Transport net = new Transport(); net.size = 1; net.unknownLength = true;
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "error");
            assertEquals(0, env.validations.get());
        }
    }
    @Test public void permissionRetryRetainsApkAndRevalidatesBeforeInstall() throws Exception {
        Env env = new Env(); Transport net = new Transport(); env.permission = false;
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "ready");
            manager.install(); await(manager, "permissionRequired"); assertEquals(2, env.validations.get());
            env.permission = true; manager.install(); long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(2); while (env.installs.get() < 2 && System.nanoTime() < until) Thread.sleep(5); await(manager, "installing"); assertEquals(3, env.validations.get());
            manager.installerResult(false); assertEquals("ready", manager.status().getString("phase")); assertTrue(manager.status().getString("error").contains("hủy"));
            assertEquals(2, net.requests.get());
        }
    }
    @Test public void modifiedApkAndWrongArchiveIdentityAreRefused() throws Exception {
        Env env = new Env(); Transport net = new Transport();
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "ready");
            Files.write(env.directory.listFiles()[0].toPath(), "changed".getBytes());
            manager.install(); await(manager, "error"); assertEquals(0, env.installs.get());
        }
        env = new Env(); env.valid = false;
        try (UpdateManager manager = new UpdateManager(env, new Transport())) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "error"); assertEquals(0, env.installs.get());
        }
    }
    @Test public void persistedThrottleAndWifiOnlyBlockNetworkButManualCanRetry() throws Exception {
        Env env = new Env(); Transport net = new Transport();
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(true); await(manager, "available");
            assertTrue(env.last > 0); manager.check(true); assertEquals(1, net.requests.get());
            manager.check(false); await(manager, "available"); assertEquals(2, net.requests.get());
            env.wifi = false; manager.download(); await(manager, "error"); assertEquals(2, net.requests.get());
        }
    }
    @Test public void cancellingReadyDiscardsVerifiedApkAndAllowsANewCheck() throws Exception {
        Env env = new Env(); Transport net = new Transport();
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "ready");
            assertEquals(1, env.directory.list().length);
            assertEquals("available", manager.cancel().getString("phase"));
            assertEquals(0, manager.status().getLong("downloadedBytes"));
            assertTrue(manager.status().isNull("totalBytes"));
            manager.check(false); await(manager, "available");
            assertEquals(3, net.requests.get()); assertEquals(0, env.directory.list().length);
        }
    }
    @Test public void failedOfflineAttemptPersistsThrottleAndLastCheckedAcrossRestart() throws Exception {
        Env env = new Env(); env.wifi = false; Transport net = new Transport();
        long checked;
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(true); checked = await(manager, "error").getLong("lastChecked");
            assertEquals(env.last, checked); assertEquals(0, net.requests.get());
        }
        env.wifi = true;
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(true);
            long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
            while (manager.status().getString("phase").equals("checking") && System.nanoTime() < end) Thread.sleep(5);
            assertEquals(checked, manager.status().getLong("lastChecked")); assertEquals(0, net.requests.get());
            manager.check(false); await(manager, "available"); assertEquals(1, net.requests.get());
        }
    }
    @Test public void cancelGuardsInstallerLaunchAlreadyQueuedOnMainThread() throws Exception {
        Env env = new Env(); env.deferInstall = true;
        try (UpdateManager manager = new UpdateManager(env, new Transport())) {
            manager.check(false); await(manager, "available"); manager.download(); await(manager, "ready"); manager.install();
            long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
            while (env.pendingInstall == null && System.nanoTime() < end) Thread.sleep(5);
            assertNotNull(env.pendingInstall); manager.cancel(); env.pendingInstall.run();
            assertEquals(0, env.installs.get()); assertEquals("available", manager.status().getString("phase"));
        }
    }
    @Test public void cancellationPreventsStaleCompletionAndRemovesStartupDebris() throws Exception {
        Env env = new Env(); Files.write(new File(env.directory, "old.part").toPath(), new byte[]{1});
        Transport net = new Transport(); net.entered = new CountDownLatch(1); net.unblock = new CountDownLatch(1);
        try (UpdateManager manager = new UpdateManager(env, net)) {
            manager.check(false); assertTrue(net.entered.await(1, TimeUnit.SECONDS));
            assertEquals(0, env.directory.list().length); manager.cancel();
            net.unblock.countDown(); assertEquals("idle", manager.status().getString("phase"));
            net.entered = null; manager.check(false); await(manager, "available");
        }
    }
}
