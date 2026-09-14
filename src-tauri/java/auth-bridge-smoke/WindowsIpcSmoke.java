import java.lang.reflect.*;
import java.nio.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.*;
import java.security.spec.X509EncodedKeySpec;
import java.time.Instant;
import java.util.*;
import java.util.regex.*;

/** Actual JNI in an AppContainer, using a public synthetic certificate and no accounts/network. */
public final class WindowsIpcSmoke {
    static Method read, write;
    static long handle, next;
    static void transfer(byte[] bytes, boolean writing) throws Exception {
        int offset = 0;
        while (offset < bytes.length) {
            int count = (int)(writing ? write : read).invoke(null, handle, bytes, offset, bytes.length-offset);
            if (count <= 0) throw new IllegalStateException("IPC closed");
            offset += count;
        }
    }
    static String rpc(String command) throws Exception {
        byte[] body = ("{\"id\":" + (++next) + ",\"command\":" + command + "}").getBytes(StandardCharsets.UTF_8);
        transfer(ByteBuffer.allocate(4).putInt(body.length).array(), true);
        transfer(body, true);
        byte[] header = new byte[4]; transfer(header, false);
        int length = ByteBuffer.wrap(header).getInt();
        if (length <= 0 || length > 262144) throw new IllegalStateException("bad frame");
        byte[] result = new byte[length]; transfer(result, false);
        return new String(result, StandardCharsets.UTF_8);
    }
    static String field(String json, String name) {
        Matcher match = Pattern.compile("\""+name+"\":\"([^\"]+)\"").matcher(json);
        if (!match.find()) throw new IllegalStateException("missing field");
        return match.group(1);
    }
    public static void main(String[] args) throws Exception {
        Class<?> io = Class.forName("me.aomona.auth.NativeIO", true, null);
        read = io.getDeclaredMethod("read", long.class, byte[].class, int.class, int.class);
        write = io.getDeclaredMethod("write", long.class, byte[].class, int.class, int.class);
        read.setAccessible(true); write.setAccessible(true);
        handle = Long.parseLong(System.getProperty("monalauncher.auth.handle"));
        if (!rpc("{\"type\":\"hello\"}").contains("\"protocol\":1")) throw new IllegalStateException("handshake");
        if (args[0].equals("denied")) {
            if (!rpc("{\"type\":\"certificate\"}").contains("\"error\":\"network_denied\"")) throw new IllegalStateException();
            System.out.println("WINDOWS_AUTH_NETWORK_DENIED_OK"); return;
        }
        String certificate = rpc("{\"type\":\"certificate\"}");
        if (certificate.contains("PRIVATE KEY")) throw new IllegalStateException("private key exposed");
        String marker = field(certificate, "privateKey");
        if (!marker.matches("MONALAUNCHER_REMOTE_CHAT_KEY:[a-f0-9]{64}")) throw new IllegalStateException();
        String id = marker.substring("MONALAUNCHER_REMOTE_CHAT_KEY:".length());
        UUID session = UUID.randomUUID();
        byte[] content = "Windows AppContainer auth probe".getBytes(StandardCharsets.UTF_8);
        byte[] message = ByteBuffer.allocate(64+content.length).putInt(1)
            .put(HexFormat.of().parseHex("0123456789abcdef0123456789abcdef"))
            .putLong(session.getMostSignificantBits()).putLong(session.getLeastSignificantBits())
            .putInt(0).putLong(123).putLong(Instant.now().getEpochSecond())
            .putInt(content.length).put(content).putInt(0).array();
        String encoded = Base64.getEncoder().encodeToString(message);
        String foreign = rpc("{\"type\":\"sign\",\"key_id\":\""+args[2]+"\",\"message\":\""+encoded+"\"}");
        if (!foreign.contains("\"error\":\"revoked\"")) throw new IllegalStateException("foreign key accepted");
        String result = rpc("{\"type\":\"sign\",\"key_id\":\""+id+"\",\"message\":\""+encoded+"\"}");
        String pem = Files.readString(Path.of(args[1])).replace("-----BEGIN PUBLIC KEY-----", "")
            .replace("-----END PUBLIC KEY-----", "").replace("-----BEGIN RSA PUBLIC KEY-----", "")
            .replace("-----END RSA PUBLIC KEY-----", "").replaceAll("\\s", "");
        PublicKey publicKey = KeyFactory.getInstance("RSA").generatePublic(new X509EncodedKeySpec(Base64.getDecoder().decode(pem)));
        Signature verifier = Signature.getInstance("SHA256withRSA"); verifier.initVerify(publicKey); verifier.update(message);
        if (!verifier.verify(Base64.getDecoder().decode(field(result,"signature")))) throw new IllegalStateException("signature mismatch");
        System.out.println("WINDOWS_AUTH_JNI_SIGNATURE_AND_FOREIGN_KEY_OK");
    }
}
