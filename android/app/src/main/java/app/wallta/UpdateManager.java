package app.wallta;

import java.io.*;
import java.security.MessageDigest;
import java.util.concurrent.*;
import java.util.function.Consumer;
import org.json.JSONObject;

/** One worker owns disk/network IO; synchronized commands only snapshot or dispatch work. */
final class UpdateManager implements AutoCloseable {
    interface Environment {
        String currentVersion(); long currentCode(); File directory(); long lastAttempt(); void recordAttempt(long time);
        boolean networkAllowed(); void validateArchive(File file, UpdatePolicy.Release release) throws Exception;
        void launchInstall(File file, java.util.function.BooleanSupplier active, Consumer<String> result);
    }
    interface Transport { Response open(String url) throws Exception; void cancel(); }
    static final class Response implements AutoCloseable {
        final InputStream stream; final long length; final Runnable cleanup;
        Response(InputStream stream, long length, Runnable cleanup) { this.stream = stream; this.length = length; this.cleanup = cleanup; }
        public void close() throws IOException { try { stream.close(); } finally { cleanup.run(); } }
    }
    private final Environment env;
    private final Transport transport;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private final String currentVersion;
    private final long currentCode;
    private Future<?> job;
    private boolean closed, busy;
    private long generation, downloaded, lastAttempt;
    private Long total, lastChecked;
    private String phase = "idle", error;
    private UpdatePolicy.Release release;
    private File ready;

    UpdateManager(Environment env, Transport transport) {
        this.env = env; this.transport = transport; currentVersion = env.currentVersion(); currentCode = env.currentCode();
        worker.execute(() -> {
            try {
                File directory = env.directory();
                if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Không tạo được thư mục cập nhật.");
                File[] files = directory.listFiles();
                if (files != null) for (File file : files) if (file.isFile()) file.delete();
                synchronized (this) { lastAttempt = env.lastAttempt(); lastChecked = lastAttempt == 0 ? null : lastAttempt; }
            } catch (Exception e) { synchronized (this) { if (!closed) { phase = "error"; error = "Không khởi tạo được bộ cập nhật."; } } }
        });
    }
    synchronized JSONObject status() {
        try { return new JSONObject().put("currentVersion", currentVersion).put("phase", phase)
            .put("version", release == null ? JSONObject.NULL : release.version).put("notes", release == null ? JSONObject.NULL : release.notes)
            .put("downloadedBytes", downloaded).put("totalBytes", total == null ? JSONObject.NULL : total)
            .put("error", error == null ? JSONObject.NULL : error).put("lastChecked", lastChecked == null ? JSONObject.NULL : lastChecked);
        } catch (org.json.JSONException e) { throw new IllegalStateException(e); }
    }
    synchronized JSONObject check(boolean automatic) {
        if (closed || busy || ready != null || "installing".equals(phase)) return status();
        if (!UpdatePolicy.checkAllowed(automatic, System.currentTimeMillis(), lastAttempt)) return status();
        String previousPhase = phase;
        dispatch("checking", token -> {
            long now = System.currentTimeMillis();
            synchronized (this) {
                ensureCurrent(token);
                if (!UpdatePolicy.checkAllowed(automatic, now, lastAttempt)) { phase = previousPhase; busy = false; return; }
                lastAttempt = now; lastChecked = now;
            }
            // Persist the attempt before IO, including failures and offline attempts.
            env.recordAttempt(now);
            requireNetwork();
            byte[] bytes;
            try (Response response = transport.open(UpdatePolicy.MANIFEST_URL)) {
                if (response.length > UpdatePolicy.MANIFEST_LIMIT) throw new IOException("Thông tin cập nhật quá lớn.");
                ByteArrayOutputStream output = new ByteArrayOutputStream();
                copy(response.stream, output, UpdatePolicy.MANIFEST_LIMIT, token, false); bytes = output.toByteArray();
            }
            UpdatePolicy.Release found = UpdatePolicy.release(new JSONObject(new String(bytes, java.nio.charset.StandardCharsets.UTF_8)), currentVersion, currentCode);
            synchronized (this) {
                ensureCurrent(token); release = found; downloaded = 0; total = found == null ? null : found.size;
                lastChecked = now; phase = found == null ? "upToDate" : "available"; busy = false;
            }
        });
        return status();
    }
    synchronized JSONObject download() {
        if (closed || busy || ready != null || release == null || "installing".equals(phase)) return status();
        UpdatePolicy.Release selected = release;
        downloaded = 0; total = selected.size;
        dispatch("downloading", token -> {
            requireNetwork();
            File partial = new File(env.directory(), "update-" + token + ".part");
            File verified = new File(env.directory(), "update-" + token + ".apk");
            boolean retained = false;
            try {
                try (Response response = transport.open(selected.url); OutputStream output = new FileOutputStream(partial)) {
                    if (response.length >= 0 && response.length != selected.size) throw new IOException("Kích thước APK không khớp.");
                    long bytes = copy(response.stream, output, Math.min(selected.size, UpdatePolicy.APK_LIMIT), token, true);
                    if (bytes != selected.size) throw new IOException("Kích thước APK không khớp.");
                }
                verifyFile(partial, selected, token);
                synchronized (this) {
                    ensureCurrent(token);
                    if (!partial.renameTo(verified)) throw new IOException("Không lưu được APK đã xác minh.");
                    ready = verified; phase = "ready"; busy = false; retained = true;
                }
            } finally { partial.delete(); if (!retained) verified.delete(); }
        });
        return status();
    }
    synchronized JSONObject cancel() {
        if (closed || (!busy && ready == null) || ("installing".equals(phase) && !busy)) return status();
        generation++; busy = false;
        if (job != null) job.cancel(true);
        transport.cancel();
        File discarded = ready; ready = null;
        if (discarded != null) worker.execute(discarded::delete);
        phase = release == null ? "idle" : "available"; downloaded = 0; total = null; error = null;
        return status();
    }
    synchronized JSONObject install() {
        if (closed || busy || ready == null || release == null || "installing".equals(phase)) return status();
        File apk = ready; UpdatePolicy.Release selected = release;
        dispatch("installing", token -> {
            try { verifyFile(apk, selected, token); }
            catch (Exception e) { apk.delete(); synchronized (this) { if (generation == token) ready = null; } throw e; }
            synchronized (this) {
                ensureCurrent(token);
                env.launchInstall(apk, () -> {
                    synchronized (UpdateManager.this) { return !closed && generation == token && busy && ready == apk; }
                }, result -> {
                    synchronized (UpdateManager.this) {
                        if (closed || generation != token) return;
                        busy = false;
                        if ("permissionRequired".equals(result) || "installing".equals(result)) { phase = result; error = null; }
                        else { phase = "ready"; error = result; }
                    }
                });
            }
        });
        return status();
    }
    synchronized void installerResult(boolean success) {
        if (closed || !"installing".equals(phase)) return;
        // A result cannot prove replacement; only the installed package version on restart does.
        phase = ready == null ? "idle" : "ready"; busy = false;
        error = success ? "Hãy mở lại Wallta để kiểm tra phiên bản đã cài." : "Đã hủy hoặc không thể cài cập nhật. Có thể thử lại.";
    }
    private void verifyFile(File file, UpdatePolicy.Release selected, long token) throws Exception {
        if (!file.isFile() || file.length() != selected.size) throw new IOException("Kích thước APK không khớp.");
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream stream = new FileInputStream(file)) {
            byte[] buffer = new byte[32768]; long count = 0; int n;
            while ((n = stream.read(buffer)) != -1) { ensureCurrent(token); count += n; if (count > selected.size || count > UpdatePolicy.APK_LIMIT) throw new IOException("APK vượt giới hạn kích thước."); digest.update(buffer, 0, n); }
            if (count != selected.size) throw new IOException("Kích thước APK không khớp.");
        }
        StringBuilder actual = new StringBuilder();
        for (byte b : digest.digest()) actual.append(String.format(java.util.Locale.ROOT, "%02x", b & 255));
        if (!selected.sha256.equals(actual.toString())) throw new IOException("SHA-256 của APK không khớp. Hãy tải lại.");
        env.validateArchive(file, selected); ensureCurrent(token);
    }
    private long copy(InputStream input, OutputStream output, long limit, long token, boolean progress) throws Exception {
        byte[] buffer = new byte[32768]; long count = 0; int n;
        while ((n = input.read(buffer)) != -1) {
            ensureCurrent(token); count += n;
            if (count > limit) throw new IOException("Tệp cập nhật vượt giới hạn kích thước.");
            if (progress) requireNetwork();
            output.write(buffer, 0, n);
            if (progress) synchronized (this) { ensureCurrent(token); downloaded = count; }
        }
        ensureCurrent(token); return count;
    }
    private void requireNetwork() throws IOException { if (!env.networkAllowed()) throw new IOException("Cần kết nối mạng phù hợp với tùy chọn Chỉ Wi-Fi."); }
    private synchronized void ensureCurrent(long token) {
        if (closed || generation != token || Thread.currentThread().isInterrupted()) throw new CancellationException();
    }
    private interface Work { void run(long token) throws Exception; }
    private void dispatch(String next, Work work) {
        long token = ++generation; phase = next; error = null; busy = true;
        job = worker.submit(() -> {
            try { ensureCurrent(token); work.run(token); }
            catch (Exception e) {
                synchronized (this) {
                    if (closed || generation != token) return;
                    busy = false; phase = "error";
                    error = e.getMessage() == null ? "Không thể cập nhật. Hãy thử lại." : e.getMessage();
                }
            }
        });
    }
    public synchronized void close() {
        if (closed) return;
        closed = true; generation++; if (job != null) job.cancel(true); transport.cancel(); worker.shutdownNow();
    }
}
