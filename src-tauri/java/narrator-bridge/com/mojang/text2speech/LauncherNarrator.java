package com.mojang.text2speech;

import java.nio.charset.StandardCharsets;
import java.util.Base64;

final class LauncherNarrator implements Narrator {
    private static final String PREFIX = "MONALAUNCHER_NARRATOR\t";

    @Override
    public void say(String text, boolean interrupt, float volume) {
        String encoded = Base64.getEncoder().encodeToString(text.getBytes(StandardCharsets.UTF_8));
        System.out.println(
            PREFIX + "SAY\t" + (interrupt ? "1" : "0") + "\t" + volume + "\t" + encoded
        );
        System.out.flush();
    }

    @Override
    public void clear() {
        System.out.println(PREFIX + "CLEAR");
        System.out.flush();
    }

    @Override
    public void destroy() {
        clear();
    }
}
