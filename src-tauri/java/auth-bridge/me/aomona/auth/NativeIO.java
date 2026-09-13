package me.aomona.auth;

public final class NativeIO {
    private NativeIO() {}
    public static void initialize(String library) { System.load(library); prepare(3); }
    static native void prepare(long handle);
    static native int read(long handle, byte[] bytes, int offset, int count);
    static native int write(long handle, byte[] bytes, int offset, int count);
}
