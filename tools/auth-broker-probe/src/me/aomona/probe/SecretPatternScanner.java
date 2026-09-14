package me.aomona.probe;

import java.io.*;
import java.security.MessageDigest;
import java.util.*;

/** Rolling hash filters candidates; SHA-256 confirms them. Stores fingerprints, never secrets. */
final class SecretPatternScanner {
    private record Pattern(String name, long rolling, byte[] sha256) {}
    private static final class Window {
        final byte[] ring;
        final List<Pattern> patterns = new ArrayList<>();
        final long power;
        int offset, filled;
        long rolling;
        Window(int length) {
            ring = new byte[length];
            long p = 1; for (int i = 0; i < length; i++) p *= 257;
            power = p;
        }
        void add(int value, MessageDigest sha, Set<String> matches) {
            int old = ring[offset] & 255;
            ring[offset] = (byte)value;
            offset = (offset + 1) % ring.length;
            rolling = rolling * 257 + value - old * power;
            if (filled < ring.length) filled++;
            if (filled < ring.length) return;
            for (Pattern pattern : patterns) {
                if (rolling != pattern.rolling || matches.contains(pattern.name)) continue;
                byte[] candidate = new byte[ring.length];
                for (int i = 0; i < candidate.length; i++) candidate[i] = ring[(offset + i) % ring.length];
                if (MessageDigest.isEqual(sha.digest(candidate), pattern.sha256)) matches.add(pattern.name);
            }
        }
    }
    private final Properties configuration;
    private final MessageDigest sha;
    SecretPatternScanner(Properties configuration) throws Exception {
        this.configuration = configuration;
        sha = MessageDigest.getInstance("SHA-256");
    }
    Set<String> scan(InputStream input) throws Exception {
        Map<Integer, Window> lengths = new TreeMap<>();
        int count = Integer.parseInt(configuration.getProperty("pattern.count"));
        if (count < 1 || count > 32) throw new IllegalArgumentException();
        for (int i = 0; i < count; i++) {
            String prefix = "pattern." + i + ".";
            int length = Integer.parseInt(configuration.getProperty(prefix + "length"));
            String name = configuration.getProperty(prefix + "name");
            byte[] fingerprint = HexFormat.of().parseHex(configuration.getProperty(prefix + "sha256"));
            if (length < 16 || length > 256 || name == null || !name.matches("[a-z0-9_]+") || fingerprint.length != 32)
                throw new IllegalArgumentException();
            lengths.computeIfAbsent(length, Window::new).patterns.add(new Pattern(name,
                Long.parseUnsignedLong(configuration.getProperty(prefix + "rolling"), 16), fingerprint));
        }
        Window[] windows = lengths.values().toArray(Window[]::new);
        Set<String> matches = new TreeSet<>();
        byte[] buffer = new byte[1024 * 1024];
        int read;
        while ((read = input.read(buffer)) != -1)
            for (int i = 0; i < read; i++) for (Window window : windows) window.add(buffer[i] & 255, sha, matches);
        return matches;
    }
    Set<String> scan(byte[] bytes) throws Exception { return scan(new ByteArrayInputStream(bytes)); }

    public static void main(String[] args) throws Exception {
        byte[] needle = new byte[32];
        for (int i = 0; i < needle.length; i++) needle[i] = (byte)(i * 71 + 13);
        long rolling = 0; for (byte value : needle) rolling = rolling * 257 + (value & 255);
        Properties config = new Properties(); config.setProperty("pattern.count", "1");
        config.setProperty("pattern.0.name", "self_test"); config.setProperty("pattern.0.length", "32");
        config.setProperty("pattern.0.rolling", Long.toUnsignedString(rolling, 16));
        config.setProperty("pattern.0.sha256", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(needle)));
        byte[] haystack = new byte[1024 * 1024 + 64];
        System.arraycopy(needle, 0, haystack, 1024 * 1024 - 11, needle.length);
        if (!new SecretPatternScanner(config).scan(haystack).contains("self_test")) throw new AssertionError("boundary match");
        config.setProperty("pattern.0.sha256", "00".repeat(32));
        if (!new SecretPatternScanner(config).scan(haystack).isEmpty()) throw new AssertionError("hash filter is not confirmation");
        System.out.println("SECRET_PATTERN_SCANNER_CONTROL_OK");
    }
}
