package app.wallta;

import static org.junit.Assert.*;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.*;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.Test;

@SuppressWarnings(
    "try") // Explicit close is the behavior under test; try-with-resources also guarantees cleanup
// on failed assertions.
public class LanTransferServerTest {
  private LanTransferServer server(File root, AtomicLong now, LanTransferServer.Limits limits)
      throws Exception {
    return new LanTransferServer(
        root,
        "<html>transfer</html>".getBytes(StandardCharsets.UTF_8),
        file -> {
          if (!Arrays.equals(Files.readAllBytes(file.toPath()), new byte[] {1, 2, 3}))
            throw new IOException("Invalid image");
          return "image/png";
        },
        Collections.singletonList("127.0.0.1"),
        now::get,
        limits);
  }

  private String request(String url, String method, String suffix, String headers, byte[] body)
      throws Exception {
    URI uri = URI.create(url);
    try (Socket socket = new Socket(uri.getHost(), uri.getPort())) {
      socket.setSoTimeout(4000);
      OutputStream out = socket.getOutputStream();
      out.write(
          (method
                  + " "
                  + uri.getRawPath()
                  + suffix
                  + " HTTP/1.1\r\nHost: "
                  + uri.getAuthority()
                  + "\r\n"
                  + headers
                  + "\r\n")
              .getBytes(StandardCharsets.US_ASCII));
      out.write(body);
      out.flush();
      socket.shutdownOutput();
      return new String(socket.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
    }
  }

  private String upload(String url, String headers, byte[] body) throws Exception {
    URI uri = URI.create(url);
    return request(
        url,
        "POST",
        "upload?name=photo.png",
        "Origin: http://" + uri.getAuthority() + "\r\n" + headers,
        body);
  }

  @Test
  public void totalRequestDeadlineReleasesTheWorkerDespiteActiveDripping() throws Exception {
    File root = Files.createTempDirectory("lan-deadline").toFile();
    try (LanTransferServer s =
        server(
            root, new AtomicLong(1000), new LanTransferServer.Limits(1024, 4096, 5, 600000, 200))) {
      s.start(Collections.emptyList());
      String url = s.status().urls.get(0);
      URI uri = URI.create(url);
      try (Socket drip = new Socket(uri.getHost(), uri.getPort())) {
        drip.getOutputStream().write('G');
        drip.getOutputStream().flush();
        Thread.sleep(80);
        drip.getOutputStream().write('E');
        drip.getOutputStream().flush();
        Thread.sleep(80);
        drip.getOutputStream().write('T');
        drip.getOutputStream().flush();
        long start = System.nanoTime();
        assertTrue(request(url, "GET", "status", "", new byte[0]).contains("200 OK"));
        assertTrue(
            "Dripping request blocked the sole worker", System.nanoTime() - start < 1_000_000_000L);
        assertTrue(s.status().active);
      }
    }
  }

  @Test
  public void validTransferIsStagedUntilExplicitAcceptanceAndExportsAreCopies() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    File original = new File(root, "original");
    Files.write(original.toPath(), new byte[] {1, 2, 3});
    try (LanTransferServer s =
        server(root, new AtomicLong(1000), LanTransferServer.Limits.standard())) {
      String id = "a".repeat(64);
      s.start(
          Collections.singletonList(
              new LanTransferServer.Export(
                  id, "photo\r\n.png", () -> new FileInputStream(original))));
      String url = s.status().urls.get(0);
      assertTrue(URI.create(url).getPath().matches("/[a-f0-9]{64}/"));
      original.delete();
      assertTrue(request(url, "GET", "files/" + id, "", new byte[0]).contains("200 OK"));
      assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1, 2, 3}).contains("200 OK"));
      assertEquals(1, s.status().receivedCount);
      assertEquals(3, s.status().receivedBytes);
      File accepted = new File(root, "accepted");
      s.accept(
          files -> {
            assertEquals(1, files.size());
            Files.copy(files.get(0).file.toPath(), accepted.toPath());
            return "saved";
          });
      assertArrayEquals(new byte[] {1, 2, 3}, Files.readAllBytes(accepted.toPath()));
      assertFalse(s.status().active);
      assertEquals(0, s.status().urls.size());
    }
  }

  @Test
  public void rejectsCapabilityHostOriginTraversalAndAmbiguousFraming() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    try (LanTransferServer s =
        server(root, new AtomicLong(1000), LanTransferServer.Limits.standard())) {
      s.start(Collections.emptyList());
      String url = s.status().urls.get(0);
      assertTrue(
          request(
                  url.replaceFirst("/[a-f0-9]{64}/$", "/" + "0".repeat(64) + "/"),
                  "GET",
                  "",
                  "",
                  new byte[0])
              .contains("404"));
      assertTrue(request(url, "GET", "", "Host: evil.invalid\r\n", new byte[0]).contains("400"));
      assertTrue(
          request(url, "POST", "upload", "Content-Length: 3\r\n", new byte[] {1, 2, 3})
              .contains("403"));
      assertTrue(request(url, "GET", "../secret", "", new byte[0]).contains("404"));
      assertTrue(
          upload(url, "Content-Length: 3\r\nContent-Length: 3\r\n", new byte[] {1, 2, 3})
              .contains("400"));
      assertTrue(
          upload(url, "Content-Length: 3\r\nTransfer-Encoding: chunked\r\n", new byte[] {1, 2, 3})
              .contains("400"));
      assertTrue(upload(url, "Content-Length: 3, 3\r\n", new byte[] {1, 2, 3}).contains("400"));
      assertEquals(0, s.status().receivedCount);
    }
  }

  @Test
  public void rejectsTruncatedInvalidAndOversizedBodiesWithoutPartialFiles() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    try (LanTransferServer s =
        server(root, new AtomicLong(1000), new LanTransferServer.Limits(3, 5, 1, 600000))) {
      s.start(Collections.emptyList());
      String url = s.status().urls.get(0);
      assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1}).contains("400"));
      assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {9, 9, 9}).contains("415"));
      assertTrue(upload(url, "Content-Length: 4\r\n", new byte[] {1, 2, 3, 4}).contains("413"));
      assertEquals(0, s.status().receivedCount);
      assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1, 2, 3}).contains("200"));
      assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1, 2, 3}).contains("413"));
      s.close();
      assertFalse(s.status().active);
      assertEquals(0, Objects.requireNonNull(root.listFiles()).length);
    }
  }

  @Test
  public void expiresAndRotatesCapabilityAndDoesNotPublishWithoutLanAddress() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    AtomicLong now = new AtomicLong(1000);
    try (LanTransferServer s = server(root, now, LanTransferServer.Limits.standard())) {
      s.start(Collections.emptyList());
      String before = s.status().urls.get(0);
      now.set(601001);
      assertFalse(s.status().active);
      assertEquals(0, s.status().expiresAt);
      s.start(Collections.emptyList());
      assertNotEquals(before, s.status().urls.get(0));
    }
    try (LanTransferServer s =
        new LanTransferServer(
            root,
            new byte[0],
            f -> "image/png",
            Collections.emptyList(),
            System::currentTimeMillis,
            LanTransferServer.Limits.standard())) {
      assertThrows(IOException.class, () -> s.start(Collections.emptyList()));
    }
  }

  @Test
  public void closeCancelsStartupWhileOriginalIsBeingRead() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    CountDownLatch opened = new CountDownLatch(1), release = new CountDownLatch(1);
    AtomicReference<Throwable> failure = new AtomicReference<>();
    try (LanTransferServer s =
        server(root, new AtomicLong(1000), LanTransferServer.Limits.standard())) {
      Thread startup =
          new Thread(
              () -> {
                try {
                  s.start(
                      Collections.singletonList(
                          new LanTransferServer.Export(
                              "b".repeat(64),
                              "photo.png",
                              () -> {
                                opened.countDown();
                                release.await();
                                return new ByteArrayInputStream(new byte[] {1, 2, 3});
                              })));
                } catch (Throwable e) {
                  failure.set(e);
                }
              });
      startup.start();
      assertTrue(opened.await(2, TimeUnit.SECONDS));
      s.close();
      release.countDown();
      startup.join(3000);
      assertFalse(startup.isAlive());
      assertNotNull(failure.get());
      assertFalse(s.status().active);
      assertEquals(0, Objects.requireNonNull(root.listFiles()).length);
    } finally {
      release.countDown();
    }
  }

  @Test
  public void totalAndCountLimitsAreIndependentAndFailedConfirmationKeepsStaging()
      throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    for (LanTransferServer.Limits limits :
        List.of(
            new LanTransferServer.Limits(3, 5, 3, 600000),
            new LanTransferServer.Limits(3, 30, 1, 600000))) {
      try (LanTransferServer s = server(root, new AtomicLong(1000), limits)) {
        s.start(Collections.emptyList());
        String url = s.status().urls.get(0);
        assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1, 2, 3}).contains("200 OK"));
        assertTrue(upload(url, "Content-Length: 3\r\n", new byte[] {1, 2, 3}).contains("413"));
        assertThrows(
            IOException.class,
            () ->
                s.accept(
                    files -> {
                      throw new IOException("Storage full");
                    }));
        assertTrue(s.status().active);
        assertEquals(1, s.status().receivedCount);
        s.accept(
            files -> {
              assertArrayEquals(
                  new byte[] {1, 2, 3}, Files.readAllBytes(files.get(0).file.toPath()));
              return null;
            });
        assertFalse(s.status().active);
      }
    }
  }

  @Test
  public void selectedExportsAreBoundedAndStopClosesInflightUpload() throws Exception {
    File root = Files.createTempDirectory("lan-test").toFile();
    try (LanTransferServer s =
        server(root, new AtomicLong(1000), new LanTransferServer.Limits(3, 5, 3, 600000))) {
      assertThrows(
          IOException.class,
          () ->
              s.start(
                  List.of(
                      new LanTransferServer.Export(
                          "c".repeat(64),
                          "large.png",
                          () -> new ByteArrayInputStream(new byte[] {1, 2, 3, 4})))));
      assertFalse(s.status().active);
      assertEquals(0, Objects.requireNonNull(root.listFiles()).length);
      assertThrows(
          IOException.class,
          () ->
              s.start(
                  List.of(
                      new LanTransferServer.Export(
                          "c".repeat(64),
                          "one.png",
                          () -> new ByteArrayInputStream(new byte[] {1, 2, 3})),
                      new LanTransferServer.Export(
                          "d".repeat(64),
                          "two.png",
                          () -> new ByteArrayInputStream(new byte[] {1, 2, 3})))));
      s.start(Collections.emptyList());
      URI uri = URI.create(s.status().urls.get(0));
      try (Socket socket = new Socket(uri.getHost(), uri.getPort())) {
        socket.setSoTimeout(4000);
        socket
            .getOutputStream()
            .write(
                ("POST "
                        + uri.getPath()
                        + "upload HTTP/1.1\r\nHost: "
                        + uri.getAuthority()
                        + "\r\nOrigin: http://"
                        + uri.getAuthority()
                        + "\r\nContent-Length: 3\r\n\r\n")
                    .getBytes(StandardCharsets.US_ASCII));
        socket.getOutputStream().flush();
        s.close();
        try {
          assertEquals(-1, socket.getInputStream().read());
        } catch (SocketException expected) {
        }
      }
      assertFalse(s.status().active);
      assertEquals(0, Objects.requireNonNull(root.listFiles()).length);
    }
  }
}
