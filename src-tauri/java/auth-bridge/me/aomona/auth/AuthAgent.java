package me.aomona.auth;

import java.lang.instrument.*;
import java.security.ProtectionDomain;

/** Compatibility adapter only: all authorization and secrets remain in Rust. */
public final class AuthAgent {
    public static void premain(String options, Instrumentation instrumentation) {
        if (options == null || options.isEmpty()) throw new IllegalArgumentException("auth bridge native library missing");
        String stage = "bootstrap";
        try {
            java.nio.file.Path bootstrap = java.nio.file.Path.of(options).getParent().resolve("auth-bootstrap.jar");
            instrumentation.appendToBootstrapClassLoaderSearch(new java.util.jar.JarFile(bootstrap.toFile()));
            stage = "native IPC";
            Class.forName("me.aomona.auth.NativeIO", true, null).getMethod("initialize", String.class).invoke(null, options);
            stage = "signature provider";
            Class.forName("me.aomona.auth.RemoteProvider", true, null).getMethod("install").invoke(null);
        } catch (ReflectiveOperationException | java.io.IOException error) {
            Throwable cause = error;
            while (cause instanceof java.lang.reflect.InvocationTargetException wrapped && wrapped.getCause() != null)
                cause = wrapped.getCause();
            String detail = cause.getClass().getSimpleName();
            // Only a fixed native diagnostic with numeric OS status may cross into the log.
            String message = cause.getMessage();
            if (cause instanceof java.io.IOException && message != null
                && message.matches("Authentication IPC prepare failed: win32=[0-9]+ type=[0-9]+")) detail += ": " + message;
            if (cause instanceof UnsatisfiedLinkError && message != null && message.contains("dependent libraries"))
                detail += ": native dependency unavailable";
            if (cause instanceof InternalError && "Error loading java.security file".equals(message))
                detail += ": Java security configuration unavailable";
            // Class/method names identify JDK initialization failures without printing paths,
            // property values or arbitrary exception messages.
            Throwable nested = cause;
            for (int depth = 0; nested != null && depth < 5; depth++, nested = nested.getCause()) {
                if (depth > 0) detail += " > " + nested.getClass().getSimpleName();
                StackTraceElement[] trace = nested.getStackTrace();
                for (int i = 0; i < Math.min(trace.length, 4); i++) {
                    if (trace[i].getClassName().startsWith("java.") || trace[i].getClassName().startsWith("sun."))
                        detail += " [" + trace[i].getClassName() + "." + trace[i].getMethodName() + ":" + trace[i].getLineNumber() + "]";
                }
            }
            throw new IllegalStateException("Authentication IPC initialization failed at " + stage + " (" + detail + ")");
        }
        instrumentation.addTransformer(new ClassFileTransformer() {
            @Override public byte[] transform(ClassLoader loader, String name, Class<?> redefined,
                                               ProtectionDomain domain, byte[] bytes) {
                boolean crypt = "net/minecraft/util/Crypt".equals(name) || "net/minecraft/class_3515".equals(name) || "bax".equals(name);
                if (!crypt && !"com/mojang/authlib/minecraft/client/MinecraftClient".equals(name)) return null;
                try {
                    byte[] adapted = crypt ? MethodAdapter.transformPrivateKeys(bytes) : MethodAdapter.transform(bytes);
                    if (crypt) System.setProperty("monalauncher.auth.chat.adapter", "crypt-v1");
                    else System.setProperty("monalauncher.auth.adapter", "authlib-client-v1");
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
