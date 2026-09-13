package me.aomona.probe;

import java.lang.reflect.*;
import java.nio.file.*;
import java.security.PrivateKey;
import java.util.concurrent.*;

/** Opt-in 26.2 test against the explicitly prepared loopback server. No credential output. */
final class OnlineConnectionProbe {
    static final String MESSAGE = "MONA_AUTH_PROBE_SIGNED_CHAT_26_2";
    interface Action { Object run() throws Exception; }

    private static Object onClient(Object client, Action action) throws Exception {
        CompletableFuture<Object> future = new CompletableFuture<>();
        client.getClass().getMethod("execute", Runnable.class).invoke(client, (Runnable) () -> {
            try { future.complete(action.run()); }
            catch (Throwable error) { future.completeExceptionally(error); }
        });
        return future.get(15, TimeUnit.SECONDS);
    }

    static void run(Path game) throws Exception {
        String phase = "startup";
        try {
            Thread.sleep(10000);
            Class<?> minecraft = Class.forName("net.minecraft.client.Minecraft");
            Object client = minecraft.getMethod("getInstance").invoke(null);
            phase = "connecting";
            onClient(client, () -> {
                Class<?> screen = Class.forName("net.minecraft.client.gui.screens.Screen");
                Class<?> address = Class.forName("net.minecraft.client.multiplayer.resolver.ServerAddress");
                Class<?> data = Class.forName("net.minecraft.client.multiplayer.ServerData");
                Class<?> type = Class.forName("net.minecraft.client.multiplayer.ServerData$Type");
                Object server = data.getConstructor(String.class, String.class, type)
                    .newInstance("Mona local auth probe", "127.0.0.1:35565", type.getField("OTHER").get(null));
                Class.forName("net.minecraft.client.gui.screens.ConnectScreen").getMethod("startConnecting",
                    screen, minecraft, address, data, boolean.class, Class.forName("net.minecraft.client.multiplayer.TransferState"))
                    .invoke(null, Class.forName("net.minecraft.client.gui.screens.TitleScreen").getConstructor().newInstance(),
                        client, address.getMethod("parseString", String.class).invoke(null, "127.0.0.1:35565"), server, false, null);
                return null;
            });
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(75);
            Object connection = null;
            while (System.nanoTime() < deadline) {
                connection = onClient(client, () -> minecraft.getField("player").get(client) == null
                    ? null : minecraft.getMethod("getConnection").invoke(client));
                if (connection != null) break;
                Thread.sleep(250);
            }
            if (connection == null) throw new IllegalStateException();
            Object liveConnection = connection;
            phase = "chat_session";
            Field sessionField = connection.getClass().getDeclaredField("chatSession");
            sessionField.setAccessible(true);
            Object session = null;
            while (System.nanoTime() < deadline) {
                session = onClient(client, () -> sessionField.get(liveConnection));
                if (session != null) break;
                Thread.sleep(250);
            }
            if (session == null) throw new IllegalStateException();
            Object pair = session.getClass().getMethod("keyPair").invoke(session);
            PrivateKey key = (PrivateKey) pair.getClass().getMethod("privateKey").invoke(pair);
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
                liveConnection.getClass().getMethod("sendChat", String.class).invoke(liveConnection, MESSAGE);
                return null;
            });
            Thread.sleep(5000);
            if (!(Boolean) onClient(client, () -> minecraft.getMethod("getConnection").invoke(client) == liveConnection))
                throw new IllegalStateException();
            Files.writeString(game.resolve("online-probe-result.json"), "{\"schema\":1,\"connected\":true,\"chatSessionOpaque\":true,\"privateKeyEncoded\":false,\"privateKeyCacheAbsent\":true,\"chatSent\":true,\"stillConnected\":true}");
            System.out.println("MONALAUNCHER_ONLINE_PROBE_COMPLETE");
        } catch (Throwable error) {
            Files.writeString(game.resolve("online-probe-result.json"), "{\"schema\":1,\"success\":false,\"phase\":\"" + phase + "\"}");
            System.err.println("MONALAUNCHER_ONLINE_PROBE_FAILED");
        }
    }
}
