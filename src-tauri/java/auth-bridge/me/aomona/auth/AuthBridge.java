package me.aomona.auth;

import java.io.IOException;
import java.lang.reflect.*;
import java.net.URL;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Map;

public final class AuthBridge {
    private static long nextId;
    private static boolean greeted;
    private static final String KEY_MARKER = "MONALAUNCHER_REMOTE_CHAT_KEY:";
    private static final java.util.Set<String> issuedKeys = java.util.concurrent.ConcurrentHashMap.newKeySet();
    private static volatile Object signingClient;
    private static volatile Object signingMapper;
    private AuthBridge() {}

    public static Object request(Object client, URL url, Object body, Class<?> responseType, int method) throws Throwable {
        Field token = client.getClass().getDeclaredField("accessToken"); token.setAccessible(true);
        boolean authenticated = token.get(client) != null;
        String address = url.toExternalForm();
        boolean join = address.equals("https://sessionserver.mojang.com/session/minecraft/join");
        if (!authenticated && !join) {
            try {
                if (method == 2) return client.getClass().getMethod("mona$original$post", URL.class, Object.class, Class.class).invoke(client, url, body, responseType);
                return client.getClass().getMethod(method == 0 ? "mona$original$get" : "mona$original$post", URL.class, Class.class).invoke(client, url, responseType);
            } catch (InvocationTargetException error) { throw error.getCause(); }
        }
        Object mapper = client.getClass().getClassLoader().loadClass("com.mojang.authlib.minecraft.client.ObjectMapper").getMethod("create").invoke(null);
        String command;
        if (join && method == 2 && body != null) {
            String hash = (String) body.getClass().getMethod("serverId").invoke(body);
            if (!hash.matches("-?[a-fA-F0-9]{1,40}")) throw failure(client, 400);
            command = "{\"type\":\"join\",\"server_hash\":\"" + hash + "\"}";
        } else if (address.equals("https://api.minecraftservices.com/player/attributes") && method == 0) {
            command = "{\"type\":\"properties\"}";
        } else if (address.equals("https://api.minecraftservices.com/privacy/blocklist") && method == 0) {
            command = "{\"type\":\"block_list\"}";
        } else if (address.equals("https://api.minecraftservices.com/player/certificates") && method != 0) {
            command = "{\"type\":\"certificate\"}";
        } else { throw failure(client, 403); }
        Object value;
        try { value = rpc(client, mapper, command); }
        catch (IOException error) { throw failure(client, 503); }
        if (value == null) return null;
        if (address.equals("https://api.minecraftservices.com/player/certificates")) {
            if (!(value instanceof Map<?,?> certificate) || !(certificate.get("keyPair") instanceof Map<?,?> pair)
                || !(pair.get("privateKey") instanceof String marker) || !marker.matches(KEY_MARKER + "[a-f0-9]{64}"))
                throw failure(client, 503);
            signingClient = client; signingMapper = mapper; issuedKeys.add(marker);
        }
        String json = (String) mapper.getClass().getMethod("writeValueAsString", Object.class).invoke(mapper, value);
        return mapper.getClass().getMethod("readValue", String.class, Class.class).invoke(mapper, json, responseType);
    }

    public static java.security.PrivateKey readPrivateKey(Class<?> crypt, String value) throws Throwable {
        if (issuedKeys.contains(value)) return new RemotePrivateKey(value.substring(KEY_MARKER.length()));
        // Old-launch markers deliberately take the game's normal invalid-cache path and refetch.
        try { return (java.security.PrivateKey) crypt.getMethod("mona$original$readPrivateKey", String.class).invoke(null, value); }
        catch (InvocationTargetException error) { throw error.getCause(); }
    }

    public static String writePrivateKey(Class<?> crypt, java.security.PrivateKey key) throws Throwable {
        if (key instanceof RemotePrivateKey remote) return KEY_MARKER + remote.id;
        try { return (String) crypt.getMethod("mona$original$writePrivateKey", java.security.PrivateKey.class).invoke(null, key); }
        catch (InvocationTargetException error) { throw error.getCause(); }
    }

    static byte[] signChat(String id, byte[] message) throws java.security.SignatureException {
        try {
            Object client = signingClient, mapper = signingMapper;
            if (client == null || mapper == null || !id.matches("[a-f0-9]{64}") || message.length > 6208)
                throw new IOException("Chat signing unavailable");
            String encoded = java.util.Base64.getEncoder().encodeToString(message);
            Object result = rpc(client, mapper, "{\"type\":\"sign\",\"key_id\":\"" + id + "\",\"message\":\"" + encoded + "\"}");
            if (!(result instanceof Map<?,?> data) || !(data.get("signature") instanceof String signature))
                throw new IOException("Chat signature unavailable");
            byte[] bytes = java.util.Base64.getDecoder().decode(signature);
            if (bytes.length != 256) throw new IOException("Chat signature size invalid");
            return bytes;
        } catch (Throwable error) { throw new java.security.SignatureException("Authentication broker refused chat signature"); }
    }

    private static RuntimeException failure(Object client, int status) throws ReflectiveOperationException {
        return (RuntimeException) client.getClass().getClassLoader()
            .loadClass("com.mojang.authlib.exceptions.MinecraftClientHttpException").getConstructor(int.class).newInstance(status);
    }

    private static synchronized Object rpc(Object client, Object mapper, String command) throws Throwable {
        if (!greeted) {
            Object hello = exchange(client, mapper, "{\"type\":\"hello\"}");
            if (!(hello instanceof Map<?,?> data) || !(data.get("protocol") instanceof Number protocol) || protocol.intValue() != 1)
                throw new IOException("Authentication protocol mismatch");
            greeted = true;
            System.setProperty("monalauncher.auth.broker.handshake", "true");
        }
        return exchange(client, mapper, command);
    }

    private static Object exchange(Object client, Object mapper, String command) throws Throwable {
        long id = ++nextId;
        byte[] request = ("{\"id\":" + id + ",\"command\":" + command + "}").getBytes(StandardCharsets.UTF_8);
        byte[] header = ByteBuffer.allocate(4).putInt(request.length).array();
        transfer(header, true); transfer(request, true); transfer(header, false);
        int length = ByteBuffer.wrap(header).getInt();
        if (length < 1 || length > 256 * 1024) throw new IOException("Authentication response size invalid");
        byte[] response = new byte[length]; transfer(response, false);
        Map<?,?> parsed = (Map<?,?>) mapper.getClass().getMethod("readValue", String.class, Class.class)
            .invoke(mapper, new String(response, StandardCharsets.UTF_8), Map.class);
        if (!(parsed.get("id") instanceof Number number) || number.longValue() != id) throw new IOException("Authentication response id mismatch");
        if (parsed.containsKey("error")) {
            int status = switch (String.valueOf(parsed.get("error"))) {
                case "invalid_request" -> 400;
                case "unauthorized", "revoked" -> 401;
                case "forbidden", "network_denied" -> 403;
                case "rate_limited" -> 429;
                case "unsupported" -> 501;
                default -> 503;
            };
            throw failure(client, status);
        }
        if (!parsed.containsKey("result")) throw new IOException("Authentication result missing");
        return parsed.get("result");
    }

    private static void transfer(byte[] bytes, boolean writing) throws IOException {
        int offset = 0;
        while (offset < bytes.length) {
            int count = writing ? NativeIO.write(NativeIO.channel(), bytes, offset, bytes.length - offset) : NativeIO.read(NativeIO.channel(), bytes, offset, bytes.length - offset);
            if (count <= 0) throw new IOException("Authentication channel closed");
            offset += count;
        }
    }
}
