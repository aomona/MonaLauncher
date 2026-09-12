import java.nio.file.*;
import java.nio.channels.*;
import java.net.*;
import java.io.*;
import java.util.*;

/** Trusted fixture. No user files or public Internet endpoints are accessed. */
public class SandboxProbe {
    interface Operation { void run() throws Exception; }
    static void check(String name, Operation operation) {
        boolean allowed = false;
        try { operation.run(); allowed = true; }
        catch (Exception error) { System.err.println(name + ": " + error); }
        System.out.println("CHECK\t" + name + "\t" + allowed);
    }
    static void writeAccess(Path path) throws Exception {
        try (var ignored = Files.newByteChannel(path, StandardOpenOption.WRITE)) {}
    }
    public static void main(String[] args) throws Exception {
        if (args.length != 4) throw new IllegalArgumentException("root tcpPort udpPort parent|leaf");
        Path root = Path.of(args[0]);
        check("read_shared", () -> Files.readAllBytes(root.resolve("shared/asset")));
        check("read_manifest", () -> Files.readAllBytes(root.resolve("instance/manifest")));
        check("write_game", () -> Files.writeString(root.resolve("game/java-write"), "probe"));
        check("read_secret", () -> Files.readAllBytes(root.resolve("host/secret")));
        check("write_secret", () -> writeAccess(root.resolve("host/secret")));
        check("read_other_instance", () -> Files.readAllBytes(root.resolve("other-instance/manifest")));
        check("write_shared", () -> writeAccess(root.resolve("shared/asset")));
        check("write_manifest", () -> writeAccess(root.resolve("instance/manifest")));
        check("read_symlink_escape", () -> Files.readAllBytes(root.resolve("game/escape/secret")));
        check("write_symlink_escape", () -> writeAccess(root.resolve("game/escape/secret")));
        check("tcp_host_loopback", () -> {
            try (Socket socket = new Socket()) {
                socket.connect(new InetSocketAddress("127.0.0.1", Integer.parseInt(args[1])), 800);
            }
        });
        check("udp_host_loopback", () -> {
            try (DatagramSocket socket = new DatagramSocket(new InetSocketAddress("127.0.0.1", 0))) {
                socket.setSoTimeout(800);
                socket.connect(InetAddress.getByName("127.0.0.1"), Integer.parseInt(args[2]));
                byte[] bytes = "probe".getBytes(java.nio.charset.StandardCharsets.UTF_8);
                socket.send(new DatagramPacket(bytes, bytes.length));
                byte[] reply = new byte[5];
                DatagramPacket packet = new DatagramPacket(reply, reply.length);
                socket.receive(packet);
                if (packet.getLength() != 5 || !Arrays.equals(bytes, reply)) throw new IOException("wrong UDP echo");
            }
        });
        check("unix_host_socket", () -> {
            try (SocketChannel socket = SocketChannel.open(StandardProtocolFamily.UNIX)) {
                socket.connect(UnixDomainSocketAddress.of(root.resolve("host/service.sock")));
            }
        });
        if (args[3].equals("parent")) {
            var child = new ProcessBuilder(
                Path.of(System.getProperty("java.home"), "bin", "java").toString(),
                "-XX:-UsePerfData", "-Djava.io.tmpdir=" + root.resolve("game"),
                "-cp", System.getProperty("java.class.path"), "SandboxProbe",
                args[0], args[1], args[2], "leaf").redirectError(ProcessBuilder.Redirect.INHERIT).start();
            try (var reader = child.inputReader()) {
                for (String line; (line = reader.readLine()) != null;) {
                    if (line.startsWith("CHECK\t")) System.out.println("CHECK\tchild." + line.substring(6));
                }
            }
            if (child.waitFor() != 0) throw new IOException("descendant JVM failed");
        } else if (!args[3].equals("leaf")) throw new IllegalArgumentException("invalid depth");
    }
}
