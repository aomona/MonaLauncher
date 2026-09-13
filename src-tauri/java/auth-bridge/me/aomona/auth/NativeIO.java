package me.aomona.auth;

public final class NativeIO {
    private NativeIO() {}
    private static long channel;
    public static void initialize(String library) {
        // A handle number is not a credential: only the specific child inherits the endpoint.
        channel = System.getProperty("os.name", "").startsWith("Windows")
            ? Long.parseLong(System.getProperty("monalauncher.auth.handle", "0")) : 3;
        if (channel <= 0) throw new IllegalStateException("Authentication IPC handle missing");
        System.load(library);
        prepare(channel);
    }
    static long channel() { return channel; }
    static native void prepare(long handle);
    static native int read(long handle, byte[] bytes, int offset, int count);
    static native int write(long handle, byte[] bytes, int offset, int count);
}
