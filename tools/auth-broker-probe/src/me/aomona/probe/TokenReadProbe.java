package me.aomona.probe;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import java.lang.management.ManagementFactory;
import com.sun.management.HotSpotDiagnosticMXBean;
import java.io.BufferedInputStream;
import java.lang.reflect.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;

/** An adversarial fixture: records detection booleans, never credential contents. */
public final class TokenReadProbe implements ClientModInitializer {
    private final Set<String> detected = new TreeSet<>();
    private final Set<String> completed = new TreeSet<>();
    private final Set<String> unavailable = new TreeSet<>();
    private long heapBytes;
    private final Set<Object> visited = Collections.newSetFromMap(new IdentityHashMap<>());
    private String expected;
    private int tokenLength;
    private int objects;

    @Override public void onInitializeClient() {
        Thread worker = new Thread(() -> {
            try {
                Path game = FabricLoader.getInstance().getGameDir();
                Properties config = new Properties();
                try (var input = Files.newInputStream(game.resolve("auth-probe.properties"))) {
                    config.load(input);
                }
                if (Boolean.parseBoolean(config.getProperty("onlineConnection", "false"))) {
                    OnlineConnectionProbe.run(game, config);
                    return;
                }
                if (Boolean.parseBoolean(config.getProperty("chatKeyProbe", "false"))) {
                    ChatKeyReadProbe.run(game, config);
                    return;
                }
                expected = config.getProperty("sha256");
                tokenLength = Integer.parseInt(config.getProperty("length"));
                if (expected == null || !expected.matches("[a-f0-9]{64}") || tokenLength < 32 || tokenLength > 32768)
                    throw new IllegalArgumentException("invalid probe configuration");
                Thread.sleep(8000);
                scan("jvm_arguments", ManagementFactory.getRuntimeMXBean().getInputArguments().toString());
                completed.add("jvm_arguments");
                Optional<String> commandLine = ProcessHandle.current().info().commandLine();
                if (commandLine.isPresent()) {
                    scan("process_arguments", commandLine.get());
                    completed.add("process_arguments");
                } else unavailable.add("process_arguments");
                for (String value : System.getenv().values()) scan("environment", value);
                completed.add("environment");
                for (Object value : System.getProperties().values()) scan("system_properties", String.valueOf(value));
                completed.add("system_properties");
                Class<?> minecraft;
                try { minecraft = Class.forName("net.minecraft.client.Minecraft"); }
                catch (ClassNotFoundException e) {
                    minecraft = Class.forName(FabricLoader.getInstance().getMappingResolver()
                        .mapClassName("intermediary", "net.minecraft.class_310"));
                }
                Object client = null;
                for (Field field : minecraft.getDeclaredFields()) {
                    if (Modifier.isStatic(field.getModifiers()) && field.getType() == minecraft) {
                        field.setAccessible(true);
                        client = field.get(null);
                        if (client != null) break;
                    }
                }
                if (client == null) throw new IllegalStateException("Minecraft singleton unavailable");
                for (Field field : minecraft.getDeclaredFields()) {
                    String type = field.getType().getName();
                    if (type.endsWith(".User") || type.endsWith(".class_320") || type.endsWith(".Session")) {
                        field.setAccessible(true);
                        Object user = field.get(client);
                        for (Field credential : user.getClass().getDeclaredFields()) {
                            if (credential.getType() == String.class && credential.trySetAccessible())
                                scan("user_session", (String) credential.get(user));
                        }
                        completed.add("user_session");
                    }
                }
                if (!completed.contains("user_session")) throw new IllegalStateException("User object unavailable");
                // Force the real game Crypt class through Fabric's loader to check the adapter's
                // mapped-name path. This loads no certificate and performs no remote operation.
                try { Class.forName("net.minecraft.util.Crypt"); }
                catch (ClassNotFoundException e) {
                    Class.forName(FabricLoader.getInstance().getMappingResolver()
                        .mapClassName("intermediary", "net.minecraft.class_3515"));
                }
                walk(client, 0);
                completed.add("game_object_fields");
                int files = 0;
                long fileBytes = 0;
                try (var paths = Files.walk(game, 4)) {
                    for (Path file : (Iterable<Path>) paths.filter(p -> Files.isRegularFile(p, LinkOption.NOFOLLOW_LINKS))::iterator) {
                        if (++files > 512) break;
                        long size = Files.size(file);
                        if (size <= 256 * 1024 && fileBytes + size <= 8 * 1024 * 1024) {
                            fileBytes += size;
                            scan("game_files", new String(Files.readAllBytes(file), StandardCharsets.UTF_8));
                        }
                    }
                }
                completed.add("game_files");
                NativeMemoryProbe.Result nativeResult = null;
                if (Boolean.parseBoolean(config.getProperty("nativeProbe", "false"))) {
                    nativeResult = NativeMemoryProbe.run(game, Integer.parseInt(config.getProperty("parentPid")),
                        Long.parseUnsignedLong(config.getProperty("parentAddress")), tokenLength);
                    if (nativeResult.parentBytes() != null)
                        scan("native_launcher_memory", new String(nativeResult.parentBytes(), StandardCharsets.US_ASCII));
                    completed.add("native_launcher_memory");
                }
                if (Boolean.parseBoolean(config.getProperty("heapDump", "true"))) scanLiveHeap(game);
                String result = "{\"schema\":1,\"fabricEntrypointRan\":true,\"tokenDetected\":" + !detected.isEmpty()
                    + ",\"legacyProfileKeyCacheVisible\":" + Files.exists(game.resolve("profilekeys/legacy-auth-probe.json"), LinkOption.NOFOLLOW_LINKS)
                    + ",\"detectedSurfaces\":" + jsonArray(detected) + ",\"completedSurfaces\":" + jsonArray(completed)
                    + ",\"unavailableSurfaces\":" + jsonArray(unavailable)
                    + ",\"objectsVisited\":" + objects + ",\"wholeHeapScanned\":false"
                    + ",\"liveJavaHeapDumpScanned\":" + completed.contains("live_java_heap_dump")
                    + ",\"heapDumpBytes\":" + heapBytes
                    + ",\"nativeMemoryProbeRan\":" + (nativeResult != null)
                    + ",\"nativeControlPassed\":" + (nativeResult != null && nativeResult.controlPassed())
                    + ",\"nativeSelfReadAllowed\":" + (nativeResult != null && nativeResult.selfReadAllowed())
                    + ",\"nativeSelfReadError\":" + (nativeResult == null ? -1 : nativeResult.selfError())
                    + ",\"launcherMemoryReadAllowed\":" + (nativeResult != null && nativeResult.parentReadAllowed())
                    + ",\"launcherMemoryReadError\":" + (nativeResult == null ? -1 : nativeResult.parentError()) + "}";
                result = result.substring(0, result.length() - 1)
                    + ",\"agentAdapterPresent\":" + "authlib-client-v1".equals(System.getProperty("monalauncher.auth.adapter"))
                    + ",\"chatAdapterPresent\":" + "crypt-v1".equals(System.getProperty("monalauncher.auth.chat.adapter"))
                    + ",\"brokerHandshakeCompleted\":" + "true".equals(System.getProperty("monalauncher.auth.broker.handshake")) + "}";
                Files.writeString(game.resolve("auth-probe-result.json"), result);
                System.out.println("MONALAUNCHER_AUTH_PROBE_COMPLETE");
            } catch (Throwable e) {
                // Exception messages / object toString() may contain credentials.
                System.err.println("MONALAUNCHER_AUTH_PROBE_FAILED:" + e.getClass().getSimpleName());
            }
        }, "mona-token-read-probe");
        worker.setDaemon(true);
        worker.start();
    }

    /** Scans all dump bytes for the hex canary in Latin-1/UTF-8 and UTF-16 BE/LE. */
    private void scanLiveHeap(Path game) throws Exception {
        Path dump = Files.createTempFile(game, "auth-probe-heap-", ".hprof");
        Files.delete(dump); // HotSpot requires the output path to be absent.
        try {
            ManagementFactory.getPlatformMXBean(HotSpotDiagnosticMXBean.class).dumpHeap(dump.toString(), true);
            heapBytes = Files.size(dump);
            byte[] ring = new byte[tokenLength * 2];
            byte[] candidate = new byte[tokenLength];
            byte[] digest = HexFormat.of().parseHex(expected);
            MessageDigest hash = MessageDigest.getInstance("SHA-256");
            int offset = 0, asciiRun = 0, previous = -1;
            int[][] utf16Runs = new int[2][2];
            long index = 0;
            try (var input = new BufferedInputStream(Files.newInputStream(dump), 1024 * 1024)) {
                int value;
                while ((value = input.read()) != -1) {
                    ring[offset] = (byte)value; offset = (offset + 1) % ring.length;
                    asciiRun = hex(value) ? asciiRun + 1 : 0;
                    if (asciiRun >= tokenLength) {
                        for (int i = 0; i < tokenLength; i++) candidate[i] = ring[(offset + tokenLength + i) % ring.length];
                        if (MessageDigest.isEqual(hash.digest(candidate), digest)) detected.add("live_java_heap_dump");
                    }
                    int parity = (int)(index++ & 1);
                    for (int endian = 0; endian < 2; endian++) {
                        boolean pair = endian == 0 ? hex(previous) && value == 0 : previous == 0 && hex(value);
                        utf16Runs[endian][parity] = pair ? utf16Runs[endian][parity] + 1 : 0;
                        if (utf16Runs[endian][parity] >= tokenLength) {
                            for (int i = 0; i < tokenLength; i++) candidate[i] = ring[(offset + 2 * i + endian) % ring.length];
                            if (MessageDigest.isEqual(hash.digest(candidate), digest)) detected.add("live_java_heap_dump");
                        }
                    }
                    previous = value;
                }
            }
            completed.add("live_java_heap_dump");
        } finally { Files.deleteIfExists(dump); }
    }
    private static boolean hex(int value) {
        return value >= '0' && value <= '9' || value >= 'a' && value <= 'f';
    }

    private void walk(Object value, int depth) throws Exception {
        if (value == null || depth > 5 || objects >= 20000 || !visited.add(value)) return;
        objects++;
        if (value instanceof String string) { scan("game_object_fields", string); return; }
        Class<?> type = value.getClass();
        if (type.isArray()) {
            if (!type.getComponentType().isPrimitive())
                for (int i = 0; i < Math.min(Array.getLength(value), 256); i++) walk(Array.get(value, i), depth + 1);
            return;
        }
        if (!type.getName().startsWith("net.minecraft.") && !type.getName().startsWith("com.mojang.authlib.")) return;
        for (Class<?> c = type; c != null && c != Object.class; c = c.getSuperclass()) {
            for (Field field : c.getDeclaredFields()) {
                if (Modifier.isStatic(field.getModifiers()) || field.getType().isPrimitive()) continue;
                if (field.trySetAccessible()) walk(field.get(value), depth + 1);
            }
        }
    }

    private void scan(String surface, String candidate) throws Exception {
        if (candidate == null || candidate.length() < tokenLength || detected.contains(surface)) return;
        MessageDigest hash = MessageDigest.getInstance("SHA-256");
        for (int i = 0; i + tokenLength <= candidate.length(); i++) {
            String digest = HexFormat.of().formatHex(hash.digest(candidate.substring(i, i + tokenLength).getBytes(StandardCharsets.UTF_8)));
            if (MessageDigest.isEqual(digest.getBytes(StandardCharsets.US_ASCII), expected.getBytes(StandardCharsets.US_ASCII))) {
                detected.add(surface); return;
            }
        }
    }

    private static String jsonArray(Collection<String> values) {
        return "[" + String.join(",", values.stream().map(s -> "\"" + s + "\"").toList()) + "]";
    }
}
