package me.aomona.probe;

import net.fabricmc.loader.api.FabricLoader;
import java.lang.reflect.*;
import java.security.PrivateKey;
import java.util.*;

/** Names pinned to the official 1.21.8 mappings and Fabric intermediary artifact. */
final class OnlineGameAdapter {
    final boolean modern;
    final Class<?> minecraft, connection, player, localChatSession, profileKeyPair;
    OnlineGameAdapter(String version) throws Exception {
        if (!Set.of("1.21.8", "26.2").contains(version)) throw new IllegalArgumentException();
        modern = version.equals("26.2");
        minecraft = type("net.minecraft.client.Minecraft", "class_310");
        connection = type("net.minecraft.client.multiplayer.ClientPacketListener", "class_634");
        player = type("net.minecraft.client.player.LocalPlayer", "class_746");
        localChatSession = type("net.minecraft.network.chat.LocalChatSession", "class_7818");
        profileKeyPair = type("net.minecraft.world.entity.player.ProfileKeyPair", "class_7427");
    }
    private Class<?> type(String named, String intermediary) throws Exception {
        return Class.forName(modern ? named : FabricLoader.getInstance().getMappingResolver()
            .mapClassName("intermediary", "net.minecraft." + intermediary));
    }
    private static Method uniqueMethod(Class<?> owner, boolean isStatic, Class<?> result, Class<?>... arguments) {
        List<Method> methods = Arrays.stream(owner.getMethods()).filter(m -> Modifier.isStatic(m.getModifiers()) == isStatic
            && !m.isBridge() && m.getReturnType() == result && Arrays.equals(m.getParameterTypes(), arguments)).toList();
        if (methods.size() != 1) throw new IllegalStateException("ambiguous adapter method");
        return methods.get(0);
    }
    private static Field uniqueField(Class<?> owner, Class<?> type) {
        List<Field> fields = Arrays.stream(owner.getDeclaredFields()).filter(f -> !Modifier.isStatic(f.getModifiers()) && f.getType() == type).toList();
        if (fields.size() != 1) throw new IllegalStateException("ambiguous adapter field");
        Field field = fields.get(0); field.setAccessible(true); return field;
    }
    Object client() throws Exception { return uniqueMethod(minecraft, true, minecraft).invoke(null); }
    Object connection(Object client) throws Exception {
        return uniqueField(minecraft, player).get(client) == null ? null : uniqueMethod(minecraft, false, connection).invoke(client);
    }
    Object chatSession(Object clientConnection) throws Exception { return uniqueField(connection, localChatSession).get(clientConnection); }
    PrivateKey privateKey(Object session) throws Exception {
        Object pair = uniqueMethod(localChatSession, false, profileKeyPair).invoke(session);
        return (PrivateKey) uniqueMethod(profileKeyPair, false, PrivateKey.class).invoke(pair);
    }
    void connect(Object client) throws Exception {
        Class<?> screen = type("net.minecraft.client.gui.screens.Screen", "class_437");
        Class<?> address = type("net.minecraft.client.multiplayer.resolver.ServerAddress", "class_639");
        Class<?> data = type("net.minecraft.client.multiplayer.ServerData", "class_642");
        Class<?> dataType = type("net.minecraft.client.multiplayer.ServerData$Type", "class_642$class_8678");
        Class<?> transfer = type("net.minecraft.client.multiplayer.TransferState", "class_9112");
        Class<?> connect = type("net.minecraft.client.gui.screens.ConnectScreen", "class_412");
        Object other = Arrays.stream(dataType.getEnumConstants()).filter(value -> ((Enum<?>)value).name().equals("OTHER")).findFirst().orElseThrow();
        Object server = data.getConstructor(String.class, String.class, dataType).newInstance("Mona local auth probe", "127.0.0.1:35565", other);
        Object parent = type("net.minecraft.client.gui.screens.TitleScreen", "class_442").getConstructor().newInstance();
        Object serverAddress = uniqueMethod(address, true, address, String.class).invoke(null, "127.0.0.1:35565");
        uniqueMethod(connect, true, void.class, screen, minecraft, address, data, boolean.class, transfer)
            .invoke(null, parent, client, serverAddress, server, false, null);
    }
    void sendChat(Object clientConnection, String message) throws Exception {
        String method = modern ? "sendChat" : FabricLoader.getInstance().getMappingResolver()
            .mapMethodName("intermediary", "net.minecraft.class_634", "method_45729", "(Ljava/lang/String;)V");
        connection.getMethod(method, String.class).invoke(clientConnection, message);
    }
}
