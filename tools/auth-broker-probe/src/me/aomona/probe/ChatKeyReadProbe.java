package me.aomona.probe;

import net.fabricmc.loader.api.FabricLoader;
import com.sun.management.HotSpotDiagnosticMXBean;
import java.lang.management.ManagementFactory;
import java.lang.reflect.*;
import java.net.*;
import java.nio.file.*;
import java.security.PrivateKey;
import java.util.*;

/** Synthetic private-key control and broker comparison inside the actual Fabric game. */
final class ChatKeyReadProbe {
    private static volatile PrivateKey liveKey;
    private static volatile String liveCacheValue;
    static Method method(Class<?> owner, Class<?> result, Class<?>... parameters) {
        return Arrays.stream(owner.getMethods()).filter(m -> Modifier.isStatic(m.getModifiers())
            && !m.getName().startsWith("mona$") && m.getReturnType() == result
            && Arrays.equals(m.getParameterTypes(), parameters)).findFirst().orElseThrow();
    }
    static void run(Path game, Properties config) throws Exception {
        String mode = config.getProperty("chatKeyMode");
        if (!Set.of("direct", "brokered").contains(mode)) throw new IllegalArgumentException();
        Thread.sleep(8000);
        Class<?> crypt;
        try { crypt = Class.forName("net.minecraft.util.Crypt"); }
        catch (ClassNotFoundException e) { crypt = Class.forName(FabricLoader.getInstance().getMappingResolver()
            .mapClassName("intermediary", "net.minecraft.class_3515")); }
        String key;
        if (mode.equals("direct")) {
            Path fixture = game.resolve("synthetic-chat-key.pem");
            key = Files.readString(fixture);
            Files.delete(fixture);
        } else {
            Class<?> clientType = Class.forName("com.mojang.authlib.minecraft.client.MinecraftClient");
            Object client = clientType.getConstructor(String.class, java.net.Proxy.class)
                .newInstance("MONALAUNCHER_BROKERED_NO_ACCESS_TOKEN", java.net.Proxy.NO_PROXY);
            Class<?> responseType = Class.forName("com.mojang.authlib.yggdrasil.response.KeyPairResponse");
            Object response = clientType.getMethod("post", URL.class, Class.class).invoke(client,
                URI.create("https://api.minecraftservices.com/player/certificates").toURL(), responseType);
            Object pair = responseType.getMethod("keyPair").invoke(response);
            key = (String) pair.getClass().getMethod("privateKey").invoke(pair);
        }
        liveKey = (PrivateKey) method(crypt, PrivateKey.class, String.class).invoke(null, key);
        liveCacheValue = (String) method(crypt, String.class, PrivateKey.class).invoke(null, liveKey);
        Files.createDirectories(game.resolve("profilekeys"));
        Files.writeString(game.resolve("profilekeys/chat-key-probe.txt"), liveCacheValue);
        SecretPatternScanner scanner = new SecretPatternScanner(config);
        byte[] encoded = liveKey.getEncoded();
        boolean encodedDetected = encoded != null && !scanner.scan(encoded).isEmpty();
        boolean cacheDetected;
        try (var input = Files.newInputStream(game.resolve("profilekeys/chat-key-probe.txt"))) {
            cacheDetected = !scanner.scan(input).isEmpty();
        }
        Path dump = Files.createTempFile(game, "auth-probe-heap-", ".hprof");
        Files.delete(dump);
        long heapBytes;
        Set<String> detected;
        try {
            ManagementFactory.getPlatformMXBean(HotSpotDiagnosticMXBean.class).dumpHeap(dump.toString(), true);
            heapBytes = Files.size(dump);
            try (var input = Files.newInputStream(dump)) { detected = scanner.scan(input); }
        } finally { Files.deleteIfExists(dump); }
        String result = "{\"schema\":1,\"fabricEntrypointRan\":true,\"liveHeapScanned\":true,\"wholeHeapScanned\":false"
            + ",\"heapBytes\":" + heapBytes + ",\"encodedPrivateKeyDetected\":" + encodedDetected
            + ",\"cachePrivateKeyDetected\":" + cacheDetected + ",\"heapPrivateKeyDetected\":" + !detected.isEmpty()
            + ",\"opaquePrivateKey\":" + liveKey.getClass().getName().equals("me.aomona.auth.RemotePrivateKey")
            + ",\"keyEncodingAvailable\":" + (encoded != null)
            + ",\"fixtureFileRemoved\":" + !Files.exists(game.resolve("synthetic-chat-key.pem"))
            + ",\"matchedRepresentations\":[" + String.join(",", detected.stream().map(name -> "\"" + name + "\"").toList()) + "]}";
        Files.writeString(game.resolve("chat-key-probe-result.json"), result);
        System.out.println("MONALAUNCHER_CHAT_KEY_PROBE_COMPLETE");
    }
}
