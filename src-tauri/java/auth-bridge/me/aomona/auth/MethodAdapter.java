package me.aomona.auth;

import java.io.*;
import java.util.*;

/** Replaces three straight-line entrypoints without embedding a second ASM on Fabric's classpath. */
final class MethodAdapter {
    private final ByteArrayOutputStream constants = new ByteArrayOutputStream();
    private final DataOutputStream cp = new DataOutputStream(constants);
    private final Map<Integer, String> utf8 = new HashMap<>();
    private int count;

    static byte[] transform(byte[] source) throws IOException { return new MethodAdapter().adapt(source); }

    private byte[] adapt(byte[] source) throws IOException {
        DataInputStream in = new DataInputStream(new ByteArrayInputStream(source));
        int magic = in.readInt();
        if (magic != 0xcafebabe) throw new IOException("invalid class");
        int minor = in.readUnsignedShort(), major = in.readUnsignedShort();
        count = in.readUnsignedShort();
        for (int i = 1; i < count; i++) {
            int tag = in.readUnsignedByte(); cp.writeByte(tag);
            switch (tag) {
                case 1: String text = in.readUTF(); cp.writeUTF(text); utf8.put(i, text); break;
                case 3: case 4: case 9: case 10: case 11: case 12: case 17: case 18: cp.write(in.readNBytes(4)); break;
                case 5: case 6: cp.write(in.readNBytes(8)); i++; break;
                case 7: case 8: case 16: case 19: case 20: cp.write(in.readNBytes(2)); break;
                case 15: cp.write(in.readNBytes(3)); break;
                default: throw new IOException("unsupported constant");
            }
        }
        ByteArrayOutputStream body = new ByteArrayOutputStream();
        DataOutputStream out = new DataOutputStream(body);
        out.write(in.readNBytes(6)); // class access, this, super
        int interfaces = in.readUnsignedShort(); out.writeShort(interfaces); out.write(in.readNBytes(interfaces * 2));
        int fields = in.readUnsignedShort(); out.writeShort(fields);
        for (int i = 0; i < fields; i++) copyMember(in, out);
        int methods = in.readUnsignedShort();
        List<byte[]> originals = new ArrayList<>(), wrappers = new ArrayList<>();
        int bridge = methodRef("me/aomona/auth/AuthBridge", "request", "(Ljava/lang/Object;Ljava/net/URL;Ljava/lang/Object;Ljava/lang/Class;I)Ljava/lang/Object;");
        int codeName = text("Code");
        for (int i = 0; i < methods; i++) {
            int flags = in.readUnsignedShort(), name = in.readUnsignedShort(), desc = in.readUnsignedShort();
            String method = utf8.get(name), descriptor = utf8.get(desc);
            int kind = -1;
            if ("get".equals(method) && "(Ljava/net/URL;Ljava/lang/Class;)Ljava/lang/Object;".equals(descriptor)) kind = 0;
            if ("post".equals(method) && "(Ljava/net/URL;Ljava/lang/Class;)Ljava/lang/Object;".equals(descriptor)) kind = 1;
            if ("post".equals(method) && "(Ljava/net/URL;Ljava/lang/Object;Ljava/lang/Class;)Ljava/lang/Object;".equals(descriptor)) kind = 2;
            ByteArrayOutputStream saved = new ByteArrayOutputStream(); DataOutputStream member = new DataOutputStream(saved);
            member.writeShort(flags); member.writeShort(kind < 0 ? name : text("mona$original$" + method)); member.writeShort(desc);
            copyAttributes(in, member);
            originals.add(saved.toByteArray());
            if (kind >= 0) {
                ByteArrayOutputStream bytes = new ByteArrayOutputStream(); DataOutputStream wrapper = new DataOutputStream(bytes);
                wrapper.writeShort(flags); wrapper.writeShort(name); wrapper.writeShort(desc); wrapper.writeShort(1);
                byte[] code = { 0x2a, 0x2b, (byte)(kind == 2 ? 0x2c : 0x01), (byte)(kind == 2 ? 0x2d : 0x2c),
                    (byte)(0x03 + kind), (byte)0xb8, (byte)(bridge >>> 8), (byte)bridge, (byte)0xb0 };
                wrapper.writeShort(codeName); wrapper.writeInt(12 + code.length);
                wrapper.writeShort(5); wrapper.writeShort(kind == 2 ? 4 : 3);
                wrapper.writeInt(code.length); wrapper.write(code); wrapper.writeShort(0); wrapper.writeShort(0);
                wrappers.add(bytes.toByteArray());
            }
        }
        if (wrappers.size() != 3) throw new IOException("authlib signature mismatch");
        out.writeShort(methods + wrappers.size());
        for (byte[] method : originals) out.write(method);
        for (byte[] method : wrappers) out.write(method);
        copyAttributes(in, out);
        if (in.available() != 0) throw new IOException("unexpected class bytes");
        ByteArrayOutputStream result = new ByteArrayOutputStream(); DataOutputStream file = new DataOutputStream(result);
        file.writeInt(magic); file.writeShort(minor); file.writeShort(major); file.writeShort(count);
        file.write(constants.toByteArray()); file.write(body.toByteArray());
        return result.toByteArray();
    }
    private int text(String value) throws IOException { int index = count++; cp.writeByte(1); cp.writeUTF(value); return index; }
    private int clazz(String name) throws IOException { int text = text(name), index = count++; cp.writeByte(7); cp.writeShort(text); return index; }
    private int methodRef(String owner, String name, String descriptor) throws IOException {
        int cls = clazz(owner), method = text(name), desc = text(descriptor), pair = count++;
        cp.writeByte(12); cp.writeShort(method); cp.writeShort(desc);
        int index = count++; cp.writeByte(10); cp.writeShort(cls); cp.writeShort(pair); return index;
    }
    private static void copyMember(DataInputStream in, DataOutputStream out) throws IOException { out.write(in.readNBytes(6)); copyAttributes(in, out); }
    private static void copyAttributes(DataInputStream in, DataOutputStream out) throws IOException {
        int count = in.readUnsignedShort(); out.writeShort(count);
        for (int i = 0; i < count; i++) {
            out.writeShort(in.readUnsignedShort()); int size = in.readInt();
            if (size < 0 || size > 16 * 1024 * 1024) throw new IOException("oversized attribute");
            out.writeInt(size); byte[] bytes = in.readNBytes(size);
            if (bytes.length != size) throw new EOFException(); out.write(bytes);
        }
    }
}
