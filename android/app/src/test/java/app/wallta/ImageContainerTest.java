package app.wallta;

import static org.junit.Assert.*;

import java.io.*;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.Base64;
import org.junit.Test;

public class ImageContainerTest {
  private void validate(byte[] bytes, String mime) throws Exception {
    File file = Files.createTempFile("wallta-container-", ".img").toFile();
    try {
      Files.write(file.toPath(), bytes);
      ImageContainer.validate(file, mime);
    } finally {
      file.delete();
    }
  }

  private byte[] png() {
    return Base64.getDecoder()
        .decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=");
  }

  @Test
  public void pngRejectsTruncationAndBrokenChunkCrc() throws Exception {
    byte[] png = png();
    validate(png, "image/png");
    assertThrows(
        IOException.class, () -> validate(Arrays.copyOf(png, png.length - 1), "image/png"));
    png[45] ^= 1;
    assertThrows(IOException.class, () -> validate(png, "image/png"));
  }

  @Test
  public void jpegRequiresCompleteMainImageButAllowsMotionPhotoTrailer() throws Exception {
    // Synthetic framing: SOI, segment, SOS, entropy (escaped FF/restart), EOI, trailer.
    byte[] jpeg = {
      (byte) 255, (byte) 216, (byte) 255, (byte) 224, 0, 4, 1, 2, (byte) 255, (byte) 218, 0, 2, 1,
      (byte) 255, 0, 2, (byte) 255, (byte) 208, 3, (byte) 255, (byte) 217, 9, 8, 7
    };
    validate(jpeg, "image/jpeg");
    assertThrows(IOException.class, () -> validate(Arrays.copyOf(jpeg, 20), "image/jpeg"));
    byte[] thumbnailOnly = {
      (byte) 255, (byte) 216, (byte) 255, (byte) 224, 0, 6, (byte) 255, (byte) 217, 1, 2
    };
    assertThrows(IOException.class, () -> validate(thumbnailOnly, "image/jpeg"));
  }

  @Test
  public void webpRequiresDeclaredRiffAndChunkSizesIncludingPadding() throws Exception {
    byte[] webp = {
      'R', 'I', 'F', 'F', 16, 0, 0, 0, 'W', 'E', 'B', 'P', 'V', 'P', '8', ' ', 3, 0, 0, 0, 1, 2, 3,
      0
    };
    validate(webp, "image/webp");
    assertThrows(IOException.class, () -> validate(Arrays.copyOf(webp, 23), "image/webp"));
    webp[16] = 5;
    assertThrows(IOException.class, () -> validate(webp, "image/webp"));
  }
}
