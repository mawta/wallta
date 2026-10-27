package app.wallta;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.SecureRandom;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.LongSupplier;

/** Session-scoped HTTP transport, independent of Android for real socket tests. */
public final class LanTransferServer implements AutoCloseable {
  public interface Validator {
    String validate(File file) throws Exception;
  }

  public interface Opener {
    InputStream open() throws Exception;
  }

  public interface Acceptor<T> {
    T accept(List<Received> files) throws Exception;
  }

  public static final class Limits {
    final long fileBytes, totalBytes, durationMillis;
    final int count;

    public Limits(long fileBytes, long totalBytes, int count, long durationMillis) {
      this.fileBytes = fileBytes;
      this.totalBytes = totalBytes;
      this.count = count;
      this.durationMillis = durationMillis;
    }

    public static Limits standard() {
      return new Limits(25L * 1024 * 1024, 256L * 1024 * 1024, 50, 600000);
    }
  }

  public static final class Export {
    public final String id, name;
    final Opener opener;

    public Export(String id, String name, Opener opener) {
      this.id = id;
      this.name = name;
      this.opener = opener;
    }
  }

  public static final class Received {
    public final File file;
    public final String name;

    Received(File file, String name) {
      this.file = file;
      this.name = name;
    }
  }

  public static final class Status {
    public boolean active;
    public List<String> urls = Collections.emptyList();
    public long expiresAt, receivedBytes;
    public int receivedCount;
    public List<Outgoing> outgoing = Collections.emptyList();

    public String json() {
      StringBuilder b = new StringBuilder("{\"active\":" + active + ",\"urls\":[");
      for (int i = 0; i < urls.size(); i++) {
        if (i > 0) b.append(',');
        b.append(quote(urls.get(i)));
      }
      b.append("],\"expiresAt\":")
          .append(expiresAt)
          .append(",\"receivedCount\":")
          .append(receivedCount)
          .append(",\"receivedBytes\":")
          .append(receivedBytes)
          .append(",\"outgoing\":[");
      for (int i = 0; i < outgoing.size(); i++) {
        if (i > 0) b.append(',');
        Outgoing f = outgoing.get(i);
        b.append("{\"id\":")
            .append(quote(f.id))
            .append(",\"name\":")
            .append(quote(f.name))
            .append(",\"size\":")
            .append(f.size)
            .append('}');
      }
      return b.append("]}").toString();
    }
  }

  public static final class Outgoing {
    public final String id, name;
    public final long size;
    final File file;
    final String mime;

    Outgoing(Export source, File file, String mime) {
      id = source.id;
      name = safeName(source.name);
      size = file.length();
      this.file = file;
      this.mime = mime;
    }
  }

  private static final class Session {
    final Object gate = new Object();
    final File folder;
    final ServerSocket listener;
    final String token;
    final long expiry;
    final List<String> urls = new ArrayList<>();
    final Set<String> hosts = new HashSet<>();
    final List<Outgoing> outgoing = new ArrayList<>();
    final List<Received> received = new ArrayList<>();
    volatile boolean active = true;
    boolean ready = false;
    volatile Socket client;
    Thread worker;
    Timer timer;
    long bytes;

    Session(File folder, ServerSocket listener, String token, long expiry) {
      this.folder = folder;
      this.listener = listener;
      this.token = token;
      this.expiry = expiry;
    }
  }

  private final File root;
  private final byte[] page;
  private final Validator validator;
  private final List<String> addresses;
  private final LongSupplier clock;
  private final Limits limits;
  private volatile Session session;
  private final java.util.concurrent.atomic.AtomicLong generation =
      new java.util.concurrent.atomic.AtomicLong();

  public LanTransferServer(
      File root,
      byte[] page,
      Validator validator,
      List<String> addresses,
      LongSupplier clock,
      Limits limits) {
    this.root = root;
    this.page = page.clone();
    this.validator = validator;
    this.addresses = new ArrayList<>(addresses);
    this.clock = clock;
    this.limits = limits;
  }

  public synchronized void start(List<Export> exports) throws Exception {
    close();
    long startGeneration = generation.get();
    if (addresses.isEmpty())
      throw new IOException(
          "No private Wi-Fi/LAN IPv4 address is available. Connect to Wi-Fi and try again.");
    if (exports.size() > limits.count) throw new IOException("Choose at most 50 photos");
    if (!root.isDirectory() && !root.mkdirs())
      throw new IOException("Cannot create transfer staging");
    File folder = Files.createTempDirectory(root.toPath(), "session-").toFile();
    ServerSocket listener = new ServerSocket();
    Session s = null;
    try {
      byte[] random = new byte[32];
      new SecureRandom().nextBytes(random);
      StringBuilder token = new StringBuilder();
      for (byte b : random) token.append(String.format(Locale.ROOT, "%02x", b & 255));
      s =
          new Session(
              folder, listener, token.toString(), clock.getAsLong() + limits.durationMillis);
      session = s;
      if (generation.get() != startGeneration) throw new IOException("Transfer startup cancelled");
      long total = 0;
      Set<String> ids = new HashSet<>();
      for (Export e : exports) {
        if (!e.id.matches("[a-f0-9]{64}") || !ids.add(e.id))
          throw new IOException("Invalid selected image ID");
        File copy = new File(folder, "out-" + e.id);
        requireActive(s);
        try (InputStream in = e.opener.open();
            OutputStream out = new FileOutputStream(copy)) {
          total += copyBounded(in, out, limits.fileBytes, s);
        }
        if (total > limits.totalBytes) throw new IOException("Selected photos exceed 256 MiB");
        s.outgoing.add(new Outgoing(e, copy, validator.validate(copy)));
      }
      synchronized (s.gate) {
        requireActive(s);
        // Do not open a listener while originals are still being read/copied.
        listener.bind(new InetSocketAddress(InetAddress.getByName("0.0.0.0"), 0));
        for (String ip : addresses) {
          if (!privateIpv4(ip) && !"127.0.0.1".equals(ip))
            throw new IOException("Invalid LAN address");
          String host = ip + ":" + listener.getLocalPort();
          s.hosts.add(host);
          s.urls.add("http://" + host + "/" + s.token + "/");
        }
        Session running = s;
        s.ready = true;
        s.worker = new Thread(() -> serve(running), "wallta-lan");
        s.worker.setDaemon(true);
        s.timer = new Timer("wallta-lan-expiry", true);
        s.timer.schedule(
            new TimerTask() {
              public void run() {
                stop(running);
              }
            },
            Math.max(1, s.expiry - clock.getAsLong()));
        s.worker.start();
      }
    } catch (Exception e) {
      listener.close();
      if (s != null) {
        s.active = false;
        if (session == s) session = null;
      }
      delete(folder);
      throw e;
    }
  }

  public Status status() {
    Session s = session;
    if (s == null) return new Status();
    if (clock.getAsLong() >= s.expiry) {
      stop(s);
      return new Status();
    }
    synchronized (s.gate) {
      return snapshot(s);
    }
  }

  private Status snapshot(Session s) {
    Status r = new Status();
    if (!s.active || !s.ready) return r;
    r.active = true;
    r.urls = new ArrayList<>(s.urls);
    r.expiresAt = s.expiry;
    r.receivedBytes = s.bytes;
    r.receivedCount = s.received.size();
    r.outgoing = new ArrayList<>(s.outgoing);
    return r;
  }

  public <T> T accept(Acceptor<T> acceptor) throws Exception {
    Session s = session;
    if (s == null) throw new IOException("Start a transfer session first");
    T result;
    synchronized (s.gate) {
      requireActive(s);
      if (s.received.isEmpty()) throw new IOException("No received photos to add");
      result = acceptor.accept(Collections.unmodifiableList(new ArrayList<>(s.received)));
      s.active = false;
    }
    stop(s);
    return result;
  }

  public void close() {
    generation.incrementAndGet();
    Session s = session;
    if (s != null) stop(s);
  }

  private void stop(Session s) {
    synchronized (s.gate) {
      s.active = false;
      if (session == s) session = null;
    }
    if (s.timer != null) s.timer.cancel();
    try {
      s.listener.close();
    } catch (IOException ignored) {
    }
    Socket client = s.client;
    if (client != null)
      try {
        client.close();
      } catch (IOException ignored) {
      }
    // Wait for the sole bounded worker before deleting open staging files (Windows included).
    if (s.worker != null && s.worker != Thread.currentThread())
      try {
        s.worker.join(3500);
      } catch (InterruptedException e) {
        Thread.currentThread().interrupt();
      }
    delete(s.folder);
  }

  private void serve(Session s) {
    try {
      while (s.active) {
        try (Socket socket = s.listener.accept()) {
          s.client = socket;
          if (!s.active) break;
          socket.setSoTimeout(2000);
          try {
            handle(s, socket);
          } catch (HttpError e) {
            respond(
                socket,
                e.status,
                "application/json",
                ("{\"error\":" + quote(e.getMessage()) + "}").getBytes(StandardCharsets.UTF_8),
                null);
          } catch (Exception e) {
            try {
              respond(
                  socket,
                  400,
                  "application/json",
                  "{\"error\":\"Invalid request\"}".getBytes(StandardCharsets.UTF_8),
                  null);
            } catch (IOException ignored) {
            }
          } finally {
            s.client = null;
          }
        } catch (IOException e) {
          if (!s.active) break;
        }
      }
    } finally {
      if (!s.active) delete(s.folder);
    }
  }

  private void requireActive(Session s) throws HttpError {
    if (!s.active || clock.getAsLong() >= s.expiry)
      throw new HttpError(410, "Transfer session expired");
  }

  private void handle(Session s, Socket socket) throws Exception {
    synchronized (s.gate) {
      requireActive(s);
    }
    InputStream in = socket.getInputStream();
    String request = line(in, 4096);
    String[] parts = request.split(" ", -1);
    if (parts.length != 3 || !parts[2].equals("HTTP/1.1") || !parts[1].startsWith("/"))
      throw new HttpError(400, "Invalid request line");
    Map<String, List<String>> headers = new HashMap<>();
    int consumed = request.length();
    while (true) {
      String h = line(in, 8192);
      consumed += h.length() + 2;
      if (consumed > 16384) throw new HttpError(400, "Headers too large");
      if (h.isEmpty()) break;
      int colon = h.indexOf(':');
      if (colon < 1 || !h.substring(0, colon).matches("[A-Za-z0-9-]+"))
        throw new HttpError(400, "Invalid header");
      String key = h.substring(0, colon).toLowerCase(Locale.ROOT);
      headers.computeIfAbsent(key, k -> new ArrayList<>()).add(h.substring(colon + 1).trim());
    }
    String host = one(headers, "host");
    if (!s.hosts.contains(host)) throw new HttpError(400, "Invalid Host");
    if (headers.containsKey("transfer-encoding"))
      throw new HttpError(400, "Transfer-Encoding is unsupported");
    if (headers.containsKey("content-length")
        && !one(headers, "content-length").matches("0|[1-9][0-9]{0,9}"))
      throw new HttpError(400, "Invalid Content-Length");
    String prefix = "/" + s.token + "/";
    String target = parts[1];
    if (!target.startsWith(prefix)) throw new HttpError(404, "Unknown route");
    String route = target.substring(prefix.length());
    String method = parts[0];
    if (method.equals("GET")) {
      if (headers.containsKey("content-length") && !one(headers, "content-length").equals("0"))
        throw new HttpError(400, "GET body is unsupported");
      if (route.isEmpty()) {
        respond(socket, 200, "text/html; charset=utf-8", page, null);
        return;
      }
      if (route.equals("status")) {
        respond(
            socket,
            200,
            "application/json",
            status().json().getBytes(StandardCharsets.UTF_8),
            null);
        return;
      }
      if (route.matches("files/[a-f0-9]{64}"))
        for (Outgoing f : s.outgoing)
          if (route.equals("files/" + f.id)) {
            try (InputStream source = new FileInputStream(f.file)) {
              responseHeaders(
                  socket,
                  200,
                  f.mime,
                  f.size,
                  "attachment; filename=\"photo"
                      + extension(f.mime)
                      + "\"; filename*=UTF-8''"
                      + encode(f.name));
              byte[] buffer = new byte[32768];
              for (int n; (n = source.read(buffer)) != -1; )
                socket.getOutputStream().write(buffer, 0, n);
              socket.getOutputStream().flush();
            }
            return;
          }
      throw new HttpError(404, "Unknown route");
    }
    if (!method.equals("POST") || !(route.equals("upload") || route.startsWith("upload?")))
      throw new HttpError(404, "Unknown route");
    if (!headers.containsKey("origin") || !one(headers, "origin").equals("http://" + host))
      throw new HttpError(403, "Same-origin upload required");
    long length;
    try {
      length = Long.parseLong(one(headers, "content-length"));
    } catch (Exception e) {
      throw new HttpError(400, "One Content-Length is required");
    }
    synchronized (s.gate) {
      requireActive(s);
      if (length < 1
          || length > limits.fileBytes
          || s.received.size() >= limits.count
          || s.bytes + length > limits.totalBytes)
        throw new HttpError(413, "Transfer limit reached");
    }
    String name = "Photo";
    if (route.startsWith("upload?")) {
      String query = route.substring(7);
      if (!query.startsWith("name=") || query.contains("&"))
        throw new HttpError(400, "Invalid upload name");
      try {
        name = URLDecoder.decode(query.substring(5), "UTF-8");
      } catch (Exception e) {
        throw new HttpError(400, "Invalid upload name");
      }
    }
    File file = File.createTempFile("incoming-", ".img", s.folder);
    boolean kept = false;
    try {
      try (OutputStream out = new FileOutputStream(file)) {
        byte[] buffer = new byte[32768];
        long remaining = length;
        while (remaining > 0) {
          int n = in.read(buffer, 0, (int) Math.min(buffer.length, remaining));
          if (n < 0) throw new HttpError(400, "Truncated upload");
          out.write(buffer, 0, n);
          remaining -= n;
        }
      }
      try {
        validator.validate(file);
      } catch (Exception e) {
        throw new HttpError(415, "Only valid JPEG, PNG or WebP images up to 120 MP are supported");
      }
      synchronized (s.gate) {
        requireActive(s);
        s.received.add(new Received(file, safeName(name)));
        s.bytes += length;
        kept = true;
      }
      respond(
          socket, 200, "application/json", "{\"ok\":true}".getBytes(StandardCharsets.UTF_8), null);
    } finally {
      if (!kept) file.delete();
    }
  }

  private static String one(Map<String, List<String>> headers, String key) throws HttpError {
    List<String> v = headers.get(key);
    if (v == null || v.size() != 1) throw new HttpError(400, "Exactly one " + key + " required");
    return v.get(0);
  }

  private static String line(InputStream in, int max) throws IOException {
    ByteArrayOutputStream b = new ByteArrayOutputStream();
    for (int i = 0; i < max; i++) {
      int c = in.read();
      if (c < 0) throw new HttpError(400, "Incomplete headers");
      if (c == 13) {
        if (in.read() != 10) throw new HttpError(400, "Invalid line ending");
        return b.toString("US-ASCII");
      }
      if (c < 32 || c > 126) throw new HttpError(400, "Invalid header bytes");
      b.write(c);
    }
    throw new HttpError(400, "Line too long");
  }

  private static void respond(
      Socket socket, int status, String type, byte[] bytes, String attachment) throws IOException {
    responseHeaders(socket, status, type, bytes.length, attachment);
    socket.getOutputStream().write(bytes);
    socket.getOutputStream().flush();
  }

  private static void responseHeaders(
      Socket socket, int status, String type, long length, String attachment) throws IOException {
    String reason =
        status == 200
            ? "OK"
            : status == 404
                ? "Not Found"
                : status == 403
                    ? "Forbidden"
                    : status == 413
                        ? "Payload Too Large"
                        : status == 415
                            ? "Unsupported Media Type"
                            : status == 410 ? "Gone" : "Bad Request";
    String h =
        "HTTP/1.1 "
            + status
            + " "
            + reason
            + "\r\nContent-Type: "
            + type
            + "\r\nContent-Length: "
            + length
            + "\r\n"
            + "Connection: close\r\n"
            + "Cache-Control: no-store\r\n"
            + "X-Content-Type-Options: nosniff\r\n"
            + "X-Frame-Options: DENY\r\n"
            + "Referrer-Policy: no-referrer\r\n"
            + "Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src"
            + " 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none';"
            + " object-src 'none'; base-uri 'none'; form-action 'none'\r\n"
            + (attachment == null ? "" : "Content-Disposition: " + attachment + "\r\n")
            + "\r\n";
    socket.getOutputStream().write(h.getBytes(StandardCharsets.US_ASCII));
  }

  private static long copyBounded(InputStream in, OutputStream out, long limit, Session s)
      throws IOException {
    byte[] b = new byte[32768];
    long total = 0;
    for (int n; (n = in.read(b)) != -1; ) {
      if (!s.active || Thread.currentThread().isInterrupted())
        throw new IOException("Transfer startup cancelled");
      total += n;
      if (total > limit) throw new IOException("Photo exceeds 25 MiB");
      out.write(b, 0, n);
    }
    if (total == 0) throw new IOException("Empty image");
    return total;
  }

  public static boolean privateIpv4(String ip) {
    String[] p = ip.split("\\.");
    if (p.length != 4) return false;
    int[] n = new int[4];
    try {
      for (int i = 0; i < 4; i++) {
        if (!p[i].matches("0|[1-9][0-9]{0,2}")) return false;
        n[i] = Integer.parseInt(p[i]);
        if (n[i] > 255) return false;
      }
    } catch (Exception e) {
      return false;
    }
    return n[0] == 10 || (n[0] == 172 && n[1] >= 16 && n[1] <= 31) || (n[0] == 192 && n[1] == 168);
  }

  public static String safeName(String value) {
    String name = value == null ? "Photo" : value.replaceAll("[\\p{Cntrl}/\\\\]", "_").trim();
    if (name.isEmpty()) name = "Photo";
    return name.substring(0, Math.min(180, name.length()));
  }

  private static String extension(String mime) {
    return mime.equals("image/jpeg") ? ".jpg" : mime.equals("image/webp") ? ".webp" : ".png";
  }

  private static String encode(String value) throws UnsupportedEncodingException {
    return URLEncoder.encode(value, "UTF-8").replace("+", "%20").replace("*", "%2A");
  }

  private static String quote(String value) {
    StringBuilder b = new StringBuilder("\"");
    for (char c : value.toCharArray()) {
      if (c == '"' || c == '\\') b.append('\\').append(c);
      else if (c < 32) b.append(String.format(Locale.ROOT, "\\u%04x", (int) c));
      else b.append(c);
    }
    return b.append('"').toString();
  }

  private static void delete(File file) {
    File[] children = file.listFiles();
    if (children != null) for (File child : children) delete(child);
    file.delete();
  }

  private static final class HttpError extends IOException {
    private static final long serialVersionUID = 1L;
    final int status;

    HttpError(int status, String message) {
      super(message);
      this.status = status;
    }
  }
}
