package me.aomona.probe;

import net.fabricmc.loader.api.FabricLoader;
import java.lang.reflect.*;
import java.net.*;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.*;
import java.time.Instant;
import java.util.*;

/** Two-game test with public synthetic keys. Forgeries deliberately bypass Java marker checks. */
final class InstanceIsolationProbe {
    static void publish(Path path, String value) throws Exception {
        Path temporary = path.resolveSibling(path.getFileName() + ".tmp");
        Files.writeString(temporary, value);
        Files.move(temporary, path, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
    }
    static void await(Path path) throws Exception {
        long deadline = System.nanoTime() + 120_000_000_000L;
        while (!Files.exists(path)) {
            if (System.nanoTime() >= deadline) throw new IllegalStateException("isolation barrier timed out");
            Thread.sleep(50);
        }
    }
    static byte[] message(UUID session, int index) {
        byte[] content = "Mona instance isolation".getBytes(StandardCharsets.UTF_8);
        return ByteBuffer.allocate(64 + content.length).putInt(1)
            .put(HexFormat.of().parseHex("0123456789abcdef0123456789abcdef"))
            .putLong(session.getMostSignificantBits()).putLong(session.getLeastSignificantBits())
            .putInt(index).putLong(123).putLong(Instant.now().getEpochSecond())
            .putInt(content.length).put(content).putInt(0).array();
    }
    static byte[] sign(PrivateKey key, byte[] message) throws Exception {
        Signature signer = Signature.getInstance("SHA256withRSA");
        signer.initSign(key); signer.update(message); return signer.sign();
    }
    static void verify(PrivateKey key, PublicKey publicKey, byte[] message) throws Exception {
        byte[] signature = sign(key, message);
        Signature verifier = Signature.getInstance("SHA256withRSA");
        verifier.initVerify(publicKey); verifier.update(message);
        if (!verifier.verify(signature)) throw new IllegalStateException("signature mismatch");
    }
    static void run(Path game, Properties config) throws Exception {
        Thread.sleep(8000);
        Class<?> clientType = Class.forName("com.mojang.authlib.minecraft.client.MinecraftClient");
        Object client = clientType.getConstructor(String.class, java.net.Proxy.class)
            .newInstance("MONALAUNCHER_BROKERED_NO_ACCESS_TOKEN", java.net.Proxy.NO_PROXY);
        Class<?> responseType = Class.forName("com.mojang.authlib.yggdrasil.response.KeyPairResponse");
        Object response = clientType.getMethod("post", URL.class, Class.class).invoke(client,
            URI.create("https://api.minecraftservices.com/player/certificates").toURL(), responseType);
        Object pair = responseType.getMethod("keyPair").invoke(response);
        String marker = (String) pair.getClass().getMethod("privateKey").invoke(pair);
        String publicPem = (String) pair.getClass().getMethod("publicKey").invoke(pair);
        if (!marker.matches("MONALAUNCHER_REMOTE_CHAT_KEY:[a-f0-9]{64}")) throw new IllegalStateException();
        String ownId = marker.substring("MONALAUNCHER_REMOTE_CHAT_KEY:".length());
        Class<?> crypt;
        try { crypt = Class.forName("net.minecraft.util.Crypt"); }
        catch (ClassNotFoundException e) { crypt = Class.forName(FabricLoader.getInstance().getMappingResolver()
            .mapClassName("intermediary", "net.minecraft.class_3515")); }
        PrivateKey own = (PrivateKey) ChatKeyReadProbe.method(crypt, PrivateKey.class, String.class).invoke(null, marker);
        PublicKey publicKey = (PublicKey) ChatKeyReadProbe.method(crypt, PublicKey.class, String.class).invoke(null, publicPem);
        if (own.getEncoded() != null) throw new IllegalStateException();
        publish(game.resolve("isolation-ready.txt"), ownId);
        await(game.resolve("isolation-peer.txt"));
        String peerId = Files.readString(game.resolve("isolation-peer.txt"));
        if (!peerId.matches("[a-f0-9]{64}") || ownId.equals(peerId)) throw new IllegalStateException();
        boolean peerFileReadable;
        try { Files.readString(Path.of(config.getProperty("peerReadyPath"))); peerFileReadable = true; }
        catch (java.io.IOException denied) { peerFileReadable = false; }
        if (peerFileReadable) throw new IllegalStateException("peer game file readable");
        Constructor<?> constructor = Class.forName("me.aomona.auth.RemotePrivateKey").getDeclaredConstructor(String.class);
        constructor.setAccessible(true);
        PrivateKey forged = (PrivateKey) constructor.newInstance(peerId);
        UUID session = UUID.randomUUID();
        byte[] first = message(session, 0);
        try { sign(forged, first); throw new IllegalStateException("foreign key signed"); }
        catch (SignatureException rejected) { }
        verify(own, publicKey, first);
        publish(game.resolve("isolation-cross.json"), "{\"foreignKeyRejected\":true,\"ownSignatureVerified\":true,\"peerReadyFileReadable\":false}");
        await(game.resolve("isolation-after-peer-exit.txt"));
        verify(own, publicKey, message(session, 1));
        publish(game.resolve("isolation-survivor.json"), "{\"signatureAfterPeerExitVerified\":true}");
        await(game.resolve("isolation-revoked.txt"));
        try { sign(own, message(session, 2)); throw new IllegalStateException("revoked lease signed"); }
        catch (SignatureException rejected) { }
        publish(game.resolve("isolation-result.json"), "{\"signatureAfterRevocationRejected\":true}");
        System.out.println("MONALAUNCHER_INSTANCE_ISOLATION_COMPLETE");
    }
}
