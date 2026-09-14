package me.aomona.probe;

import java.lang.reflect.*;
import java.nio.file.*;
import java.security.PrivateKey;
import java.util.Properties;
import java.util.concurrent.*;

/** Opt-in test against the explicitly prepared loopback server. No credential output. */
final class OnlineConnectionProbe {
    interface Action { Object run() throws Exception; }

    private static Object onClient(Object client, Action action) throws Exception {
        CompletableFuture<Object> future = new CompletableFuture<>();
        client.getClass().getMethod("execute", Runnable.class).invoke(client, (Runnable) () -> {
            try { future.complete(action.run()); }
            catch (Throwable error) { future.completeExceptionally(error); }
        });
        return future.get(15, TimeUnit.SECONDS);
    }

    static void run(Path game, Properties config) throws Exception {
        String phase = "startup";
        try {
            Thread.sleep(10000);
            String version = config.getProperty("onlineVersion", "26.2");
            OnlineGameAdapter adapter = new OnlineGameAdapter(version);
            String message = "MONA_AUTH_PROBE_SIGNED_CHAT_" + version.replace('.', '_');
            Object client = adapter.client();
            phase = "connecting";
            onClient(client, () -> {
                adapter.connect(client);
                return null;
            });
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(75);
            Object connection = null;
            while (System.nanoTime() < deadline) {
                connection = onClient(client, () -> adapter.connection(client));
                if (connection != null) break;
                Thread.sleep(250);
            }
            if (connection == null) throw new IllegalStateException();
            Object liveConnection = connection;
            phase = "chat_session";
            Object session = null;
            while (System.nanoTime() < deadline) {
                session = onClient(client, () -> adapter.chatSession(liveConnection));
                if (session != null) break;
                Thread.sleep(250);
            }
            if (session == null) throw new IllegalStateException();
            PrivateKey key = adapter.privateKey(session);
            if (!key.getClass().getName().equals("me.aomona.auth.RemotePrivateKey") || key.getEncoded() != null || key.getFormat() != null)
                throw new IllegalStateException();
            phase = "cache_check";
            Path cache = game.resolve("profilekeys");
            if (Files.exists(cache)) {
                try (var files = Files.list(cache)) {
                    for (Path file : (Iterable<Path>) files::iterator) {
                        if (!Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS) || Files.size(file) > 65536)
                            throw new IllegalStateException();
                        if (Files.readString(file).contains("PRIVATE KEY")) throw new IllegalStateException();
                    }
                }
            }
            phase = "chat_send";
            onClient(client, () -> {
                adapter.sendChat(liveConnection, message);
                return null;
            });
            Thread.sleep(5000);
            if (!(Boolean) onClient(client, () -> adapter.connection(client) == liveConnection))
                throw new IllegalStateException();
            Files.writeString(game.resolve("online-probe-result.json"), "{\"schema\":1,\"connected\":true,\"chatSessionOpaque\":true,\"privateKeyEncoded\":false,\"privateKeyCacheAbsent\":true,\"chatSent\":true,\"stillConnected\":true}");
            System.out.println("MONALAUNCHER_ONLINE_PROBE_COMPLETE");
        } catch (Throwable error) {
            Files.writeString(game.resolve("online-probe-result.json"), "{\"schema\":1,\"success\":false,\"phase\":\"" + phase + "\"}");
            System.err.println("MONALAUNCHER_ONLINE_PROBE_FAILED");
        }
    }
}
