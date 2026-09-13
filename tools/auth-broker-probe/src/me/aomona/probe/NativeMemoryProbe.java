package me.aomona.probe;

import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.Arrays;

/** Read-only native tests; callers supply only the synthetic parent allocation's address. */
public final class NativeMemoryProbe {
    private static native byte[] readProcess(int pid, long address, int length);
    private static native byte[] selfRead();
    private static native byte[] partialSelfRead();
    private static native byte[] control();
    private static native int errorCode();

    public record Result(boolean controlPassed, boolean selfReadAllowed, int selfError,
                         boolean parentReadAllowed, int parentError, byte[] parentBytes) {}

    public static Result run(Path game, int parentPid, long address, int length) throws Exception {
        String name = System.mapLibraryName("mona-memory-probe");
        Path extracted = Files.createTempFile(game, "auth-probe-native-", name.substring(name.lastIndexOf('.')));
        try {
            try (InputStream input = NativeMemoryProbe.class.getResourceAsStream("/native/" + name)) {
                if (input == null) throw new IllegalStateException("Native probe not packaged");
                Files.copy(input, extracted, StandardCopyOption.REPLACE_EXISTING);
            }
            System.load(extracted.toAbsolutePath().toString());
            byte[] known = "MonaNativeMemoryControl".getBytes(StandardCharsets.US_ASCII);
            boolean controlPassed = Arrays.equals(control(), known);
            byte[] self = selfRead(); int selfError = errorCode();
            if (self != null && !Arrays.equals(self, known)) throw new IllegalStateException("Native self read corrupted");
            byte[] parent = readProcess(parentPid, address, length); int parentError = errorCode();
            return new Result(controlPassed, self != null, selfError, parent != null, parentError, parent);
        } finally { Files.deleteIfExists(extracted); }
    }

    /** Unsandboxed positive control for the exact OS memory-read API, with no external target. */
    public static void main(String[] args) throws Exception {
        System.load(Path.of(args[0]).toAbsolutePath().toString());
        if (!Arrays.equals(control(), selfRead()) || errorCode() != 0)
            throw new AssertionError("OS self-memory-read positive control failed");
        if (System.getProperty("os.name").equals("Linux")
            && !Arrays.equals("MonaPart".getBytes(StandardCharsets.US_ASCII), partialSelfRead()))
            throw new AssertionError("Partial memory read was mistaken for denial");
        System.out.println("NATIVE_MEMORY_SELF_CONTROL_OK");
    }
}
