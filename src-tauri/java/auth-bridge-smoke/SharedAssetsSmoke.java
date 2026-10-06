import java.lang.reflect.Method;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.Security;

/** Shared embedded assets against MonaLauncher's real IPC, with no accounts or service traffic. */
public final class SharedAssetsSmoke {
    static Method read, write;
    static void transfer(byte[] bytes, boolean writing) throws Exception {
        int offset = 0;
        while (offset < bytes.length) {
            int count = (int)(writing ? write : read).invoke(null, 3L, bytes, offset, bytes.length - offset);
            if (count <= 0) throw new IllegalStateException("IPC closed");
            offset += count;
        }
    }
    static String rpc(int id, String command) throws Exception {
        byte[] body = ("{\"id\":" + id + ",\"command\":{\"type\":\"" + command + "\"}}")
            .getBytes(StandardCharsets.UTF_8);
        transfer(ByteBuffer.allocate(4).putInt(body.length).array(), true);
        transfer(body, true);
        byte[] header = new byte[4];
        transfer(header, false);
        int length = ByteBuffer.wrap(header).getInt();
        if (length <= 0 || length > 262144) throw new IllegalStateException("bad response size");
        byte[] result = new byte[length];
        transfer(result, false);
        return new String(result, StandardCharsets.UTF_8);
    }
    public static void main(String[] args) throws Exception {
        for (String name : new String[] {"AuthBridge", "NativeIO", "RemotePrivateKey", "RemoteProvider", "RemoteSignature"}) {
            if (Class.forName("me.aomona.auth." + name, true, null).getClassLoader() != null)
                throw new IllegalStateException("bridge must use bootstrap loader");
        }
        if (Security.getProvider("MonaAuthBroker") == null) throw new IllegalStateException("provider missing");
        Class<?> io = Class.forName("me.aomona.auth.NativeIO", true, null);
        read = io.getDeclaredMethod("read", long.class, byte[].class, int.class, int.class);
        write = io.getDeclaredMethod("write", long.class, byte[].class, int.class, int.class);
        read.setAccessible(true);
        write.setAccessible(true);
        if (!rpc(1, "hello").contains("\"protocol\":1")) throw new IllegalStateException("handshake");
        String result = rpc(2, "properties");
        String expected = args[0].equals("allowed") ? "\"sharedAssets\":true" : "\"error\":\"network_denied\"";
        if (!result.contains(expected)) throw new IllegalStateException("unexpected broker result");
        System.out.println("SHARED_AUTH_ASSETS_OK");
    }
}
