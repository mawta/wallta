package app.wallta;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.zip.CRC32;
import java.util.zip.DataFormatException;
import java.util.zip.Inflater;

/** Container completeness guard before the platform's bounded pixel decoder. */
final class ImageContainer {
  static void validate(File file, String mime) throws IOException {
    try (DataInputStream in =
        new DataInputStream(new BufferedInputStream(new FileInputStream(file)))) {
      switch (mime) {
        case "image/png":
          png(in, file.length());
          break;
        case "image/jpeg":
          jpeg(in);
          break;
        case "image/webp":
          webp(in, file.length());
          break;
        default:
          throw new IOException("Unsupported image container");
      }
    }
  }

  private static void png(DataInputStream in, long size) throws IOException {
    Inflater pixels = new Inflater();
    try {
      png(in, size, pixels);
    } finally {
      pixels.end();
    }
  }

  private static void png(DataInputStream in, long size, Inflater pixelsStream) throws IOException {
    byte[] signature = new byte[8];
    in.readFully(signature);
    if (!Arrays.equals(signature, new byte[] {(byte) 137, 80, 78, 71, 13, 10, 26, 10}))
      throw new IOException("Invalid PNG signature");
    long remaining = size - 8;
    boolean first = true, pixels = false;
    byte[] buffer = new byte[8192];
    byte[] decoded = new byte[8192];
    long decodedBytes = 0;
    while (remaining >= 12) {
      long length = in.readInt() & 0xffffffffL;
      if (length > remaining - 12) throw new IOException("Truncated PNG chunk");
      byte[] type = new byte[4];
      in.readFully(type);
      String name = new String(type, StandardCharsets.US_ASCII);
      if (first && (!name.equals("IHDR") || length != 13))
        throw new IOException("Missing PNG header");
      first = false;
      CRC32 crc = new CRC32();
      crc.update(type);
      for (long left = length; left > 0; ) {
        int count = (int) Math.min(left, buffer.length);
        in.readFully(buffer, 0, count);
        crc.update(buffer, 0, count);
        if (name.equals("IDAT")) {
          if (pixelsStream.finished()) throw new IOException("Extra PNG pixel data");
          pixelsStream.setInput(buffer, 0, count);
          try {
            while (!pixelsStream.needsInput() && !pixelsStream.finished()) {
              int produced = pixelsStream.inflate(decoded);
              decodedBytes += produced;
              // 120 MP, at most 8 bytes/pixel plus one filter byte per row.
              if (decodedBytes > 1_080_000_000L)
                throw new IOException("PNG expansion limit exceeded");
              if (produced == 0 && !pixelsStream.needsInput() && !pixelsStream.finished())
                throw new IOException("Invalid PNG compressed stream");
            }
            if (pixelsStream.finished() && pixelsStream.getRemaining() != 0)
              throw new IOException("Extra PNG compressed data");
          } catch (DataFormatException e) {
            throw new IOException("Invalid PNG compressed pixels", e);
          }
        }
        left -= count;
      }
      if ((in.readInt() & 0xffffffffL) != crc.getValue())
        throw new IOException("Invalid PNG chunk CRC");
      remaining -= length + 12;
      if (name.equals("IDAT")) pixels = true;
      if (name.equals("IEND")) {
        if (length != 0 || !pixels || !pixelsStream.finished() || remaining != 0)
          throw new IOException("Invalid PNG end");
        return;
      }
    }
    throw new IOException("Missing PNG end");
  }

  private static void jpeg(DataInputStream in) throws IOException {
    if (in.readUnsignedShort() != 0xffd8) throw new IOException("Invalid JPEG signature");
    boolean scan = false, pixels = false;
    while (true) {
      int next = in.readUnsignedByte();
      if (scan) {
        while (next != 255) next = in.readUnsignedByte();
      } else if (next != 255) throw new IOException("Invalid JPEG marker");
      int marker;
      do {
        marker = in.readUnsignedByte();
      } while (marker == 255);
      if (scan && (marker == 0 || marker >= 0xd0 && marker <= 0xd7)) continue;
      if (marker == 0xd9 && pixels) return; // Preserve valid Motion Photo trailers after EOI.
      if (marker == 1) continue; // TEM marker has no length.
      if (marker == 0 || marker >= 0xd0 && marker <= 0xd9) throw new IOException("Incomplete JPEG");
      int length = in.readUnsignedShort();
      if (length < 2) throw new IOException("Invalid JPEG segment");
      skip(in, length - 2);
      scan = marker == 0xda;
      pixels |= scan;
    }
  }

  private static long littleInt(DataInputStream in) throws IOException {
    return Integer.reverseBytes(in.readInt()) & 0xffffffffL;
  }

  private static void webp(DataInputStream in, long size) throws IOException {
    if (in.readInt() != 0x52494646 || littleInt(in) != size - 8 || in.readInt() != 0x57454250)
      throw new IOException("Invalid WebP RIFF size");
    long remaining = size - 12;
    if (remaining < 8) throw new IOException("Missing WebP chunks");
    while (remaining > 0) {
      if (remaining < 8) throw new IOException("Truncated WebP chunk");
      in.readInt();
      long length = littleInt(in);
      long padded = length + (length & 1);
      if (padded > remaining - 8) throw new IOException("Truncated WebP data");
      skip(in, padded);
      remaining -= padded + 8;
    }
  }

  private static void skip(DataInputStream in, long length) throws IOException {
    while (length > 0) {
      long count = in.skip(length);
      if (count == 0) {
        in.readUnsignedByte();
        count = 1;
      }
      length -= count;
    }
  }
}
