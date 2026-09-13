package me.aomona.auth;

import java.lang.instrument.*;
import java.security.ProtectionDomain;

/** Compatibility adapter only: all authorization and secrets remain in Rust. */
public final class AuthAgent {
    public static void premain(String options, Instrumentation instrumentation) {
        if (options == null || options.isEmpty()) throw new IllegalArgumentException("auth bridge native library missing");
        try {
            java.nio.file.Path bootstrap = java.nio.file.Path.of(options).getParent().resolve("auth-bootstrap.jar");
            instrumentation.appendToBootstrapClassLoaderSearch(new java.util.jar.JarFile(bootstrap.toFile()));
            Class.forName("me.aomona.auth.NativeIO", true, null).getMethod("initialize", String.class).invoke(null, options);
        } catch (ReflectiveOperationException | java.io.IOException error) {
            throw new IllegalStateException("Authentication IPC initialization failed");
        }
        instrumentation.addTransformer(new ClassFileTransformer() {
            @Override public byte[] transform(ClassLoader loader, String name, Class<?> redefined,
                                               ProtectionDomain domain, byte[] bytes) {
                if (!"com/mojang/authlib/minecraft/client/MinecraftClient".equals(name)) return null;
                try {
                    byte[] adapted = MethodAdapter.transform(bytes);
                    System.setProperty("monalauncher.auth.adapter", "authlib-client-v1");
                    return adapted;
                }
                catch (Throwable error) {
                    // Returning the original class after an adapter failure would silently change
                    // behavior. Abort this JVM; never fall back to credential-bearing arguments.
                    System.err.println("MonaLauncher authentication adapter incompatible");
                    Runtime.getRuntime().halt(78);
                    return null;
                }
            }
        });
    }
}
