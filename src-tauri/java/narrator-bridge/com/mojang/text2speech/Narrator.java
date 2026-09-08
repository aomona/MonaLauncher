package com.mojang.text2speech;

public interface Narrator {
    default void say(String text) {
        say(text, false, 1.0f);
    }

    default void say(String text, boolean interrupt) {
        say(text, interrupt, 1.0f);
    }

    void say(String text, boolean interrupt, float volume);

    void clear();

    default boolean active() {
        return true;
    }

    void destroy();

    static Narrator getNarrator() {
        return new LauncherNarrator();
    }

    static void setJNAPath(String path) {
        // Older Minecraft versions configure the native narrator through this hook. The launcher
        // bridge does not load JNA, so accepting the path without using it is intentional.
    }

    final class InitializeException extends Exception {
        public InitializeException(String message) {
            super(message);
        }
    }

    final class FatalException extends RuntimeException {
        public FatalException(String message) {
            super(message);
        }
    }
}
