package app.wallta;

import java.io.IOException;
import java.util.concurrent.TimeUnit;
import okhttp3.Call;
import okhttp3.HttpUrl;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.Response;

/** Separate client: media source URLs and credential handling cannot affect update downloads. */
final class GithubUpdateTransport implements UpdateManager.Transport {
    private final OkHttpClient client = new OkHttpClient.Builder().connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS).callTimeout(5, TimeUnit.MINUTES)
        .followRedirects(false).followSslRedirects(false).build();
    private volatile Call active;

    public UpdateManager.Response open(String url) throws Exception {
        if (!url.startsWith("https://github.com/mawta/wallta/releases/") || !UpdatePolicy.redirectAllowed(url)) throw new IllegalArgumentException("Địa chỉ cập nhật không được phép.");
        String target = url;
        for (int redirects = 0; redirects <= 5; redirects++) {
            if (Thread.currentThread().isInterrupted()) throw new IOException("Đã hủy tải cập nhật.");
            if (!UpdatePolicy.redirectAllowed(target)) throw new IOException("Chuyển hướng cập nhật không được phép.");
            Call call = client.newCall(new Request.Builder().url(target).header("Accept-Encoding", "identity").header("User-Agent", "Wallta-Updater").build());
            if (UpdatePolicy.MANIFEST_URL.equals(url)) call.timeout().timeout(60, TimeUnit.SECONDS);
            active = call;
            if (Thread.currentThread().isInterrupted()) { call.cancel(); throw new IOException("Đã hủy tải cập nhật."); }
            Response response;
            try { response = call.execute(); }
            catch (Exception e) { if (active == call) active = null; throw e; }
            if (response.code() == 301 || response.code() == 302 || response.code() == 303 || response.code() == 307 || response.code() == 308) {
                try {
                    String location = response.header("Location");
                    HttpUrl next = location == null ? null : response.request().url().resolve(location);
                    if (next == null || !UpdatePolicy.redirectAllowed(next.toString())) throw new IOException("Chuyển hướng cập nhật không được phép.");
                    target = next.toString();
                } finally { response.close(); if (active == call) active = null; }
                continue;
            }
            if (response.code() != 200 || response.body() == null) { response.close(); if (active == call) active = null; throw new IOException("Không tải được cập nhật (HTTP " + response.code() + "). Hãy thử lại."); }
            return new UpdateManager.Response(response.body().byteStream(), response.body().contentLength(), () -> { response.close(); if (active == call) active = null; });
        }
        throw new IOException("Cập nhật chuyển hướng quá nhiều lần.");
    }
    public void cancel() { Call call = active; if (call != null) call.cancel(); }
}
