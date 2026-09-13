package me.aomona.authsmoke;

import java.io.ByteArrayOutputStream;
import java.lang.reflect.*;
import java.net.URL;
import java.security.*;
import java.time.Instant;
import java.util.*;

/** Runs against installed official game/authlib classes and the actual JNI broker channel. */
public final class ChatSigningSmoke {
    static void require(boolean condition, String description) {
        if (!condition) throw new AssertionError(description);
    }
    static Method method(Class<?> owner, Class<?> result, Class<?>... parameters) {
        return Arrays.stream(owner.getMethods()).filter(m -> !m.getName().startsWith("mona$") && m.getReturnType() == result
            && Arrays.equals(m.getParameterTypes(), parameters)).findFirst().orElseThrow();
    }
    public static void main(String[] args) throws Exception {
        boolean modern = args[0].equals("26.2");
        String runtimeRoot = System.getProperty("monalauncher.auth.smoke.runtimeRoot");
        if (runtimeRoot != null) require(java.nio.file.Path.of(System.getProperty("java.home")).getRoot()
            .equals(java.nio.file.Path.of(runtimeRoot).getRoot()), "runtime uses the launcher drive alias");
        Class<?> clientType = Class.forName("com.mojang.authlib.minecraft.client.MinecraftClient");
        Object client = clientType.getConstructor(String.class, java.net.Proxy.class)
            .newInstance("MONALAUNCHER_BROKERED_NO_ACCESS_TOKEN", java.net.Proxy.NO_PROXY);
        Class<?> response = Class.forName("com.mojang.authlib.yggdrasil.response.KeyPairResponse");
        Object certificate = clientType.getMethod("post", URL.class, Class.class).invoke(client,
            java.net.URI.create("https://api.minecraftservices.com/player/certificates").toURL(), response);
        Object pair = response.getMethod("keyPair").invoke(certificate);
        String marker = (String) pair.getClass().getMethod("privateKey").invoke(pair);
        String publicPem = (String) pair.getClass().getMethod("publicKey").invoke(pair);
        require(marker.matches("MONALAUNCHER_REMOTE_CHAT_KEY:[a-f0-9]{64}"), "opaque marker");
        Class<?> crypt = Class.forName(modern ? "net.minecraft.util.Crypt" : "bax");
        Method read = method(crypt, PrivateKey.class, String.class);
        Method write = method(crypt, String.class, PrivateKey.class);
        PrivateKey privateKey = (PrivateKey) read.invoke(null, marker);
        require(privateKey.getEncoded() == null && privateKey.getFormat() == null, "no private key encoding");
        require(write.invoke(null, privateKey).equals(marker), "cache contains only handle");
        try {
            read.invoke(null, "MONALAUNCHER_REMOTE_CHAT_KEY:" + "0".repeat(64));
            throw new AssertionError("old launch marker accepted");
        } catch (InvocationTargetException expected) { }
        PublicKey publicKey = (PublicKey) method(crypt, PublicKey.class, String.class).invoke(null, publicPem);
        Class<?> linkType = Class.forName(modern ? "net.minecraft.network.chat.SignedMessageLink" : "yj");
        Class<?> bodyType = Class.forName(modern ? "net.minecraft.network.chat.SignedMessageBody" : "yh");
        Class<?> seenType = Class.forName(modern ? "net.minecraft.network.chat.LastSeenMessages" : "xv");
        Class<?> outputType = Class.forName(modern ? "net.minecraft.util.SignatureUpdater$Output" : "bcp$a");
        Class<?> messageType = Class.forName(modern ? "net.minecraft.network.chat.PlayerChatMessage" : "ye");
        Class<?> signerType = Class.forName(modern ? "net.minecraft.util.Signer" : "bcr");
        Object link = linkType.getConstructor(int.class, UUID.class, UUID.class).newInstance(0,
            UUID.fromString("01234567-89ab-cdef-0123-456789abcdef"), UUID.randomUUID());
        Object seen = seenType.getConstructor(List.class).newInstance(List.of());
        Object body = bodyType.getConstructor(String.class, Instant.class, long.class, seenType)
            .newInstance("合成鍵での検証", Instant.now(), 123L, seen);
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        Object output = java.lang.reflect.Proxy.newProxyInstance(outputType.getClassLoader(), new Class<?>[] { outputType },
            (proxy, m, arguments) -> { bytes.write((byte[]) arguments[0]); return null; });
        method(messageType, void.class, outputType, linkType, bodyType).invoke(null, output, link, body);
        Object signer = method(signerType, signerType, PrivateKey.class, String.class).invoke(null, privateKey, "SHA256withRSA");
        byte[] signature = (byte[]) method(signerType, byte[].class, byte[].class).invoke(signer, bytes.toByteArray());
        require(signature != null && signature.length == 256, "game signer used broker");
        Signature verifier = Signature.getInstance("SHA256withRSA"); verifier.initVerify(publicKey);
        verifier.update(bytes.toByteArray()); require(verifier.verify(signature), "signature verifies with public key");
        Signature replay = Signature.getInstance("SHA256withRSA"); replay.initSign(privateKey); replay.update(bytes.toByteArray());
        try { replay.sign(); throw new AssertionError("replayed index accepted"); }
        catch (SignatureException expected) { }
        KeyPairGenerator generator = KeyPairGenerator.getInstance("RSA"); generator.initialize(2048);
        KeyPair ordinary = generator.generateKeyPair(); Signature local = Signature.getInstance("SHA256withRSA");
        local.initSign(ordinary.getPrivate()); require(!local.getProvider().getName().equals("MonaAuthBroker"), "normal RSA provider preserved");
        local.update(new byte[] { 1 }); byte[] localSignature = local.sign();
        Signature localVerifier = Signature.getInstance("SHA256withRSA"); localVerifier.initVerify(ordinary.getPublic());
        localVerifier.update(new byte[] { 1 }); require(localVerifier.verify(localSignature), "normal RSA remains usable");
        require("crypt-v1".equals(System.getProperty("monalauncher.auth.chat.adapter")), "Crypt transformed");
        System.out.println("CHAT_SIGNING_SMOKE_OK " + args[0]);
    }
}
