package me.aomona.auth;

import java.security.*;
import java.util.*;

public final class RemoteProvider extends Provider {
    public RemoteProvider() {
        super("MonaAuthBroker", "1.0", "Launch-local Minecraft chat signatures");
        putService(new Service(this, "Signature", "SHA256withRSA", RemoteSignature.class.getName(),
            List.of(), Map.of("SupportedKeyClasses", RemotePrivateKey.class.getName())));
    }
    public static void install() { Security.addProvider(new RemoteProvider()); }
}
