package com.mojang.text2speech;

public interface Narrator {
    void say(String text, boolean interrupt, float volume);

    void clear();

    default boolean active() {
        return true;
    }

    void destroy();

    static Narrator getNarrator() {
        return new LauncherNarrator();
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
