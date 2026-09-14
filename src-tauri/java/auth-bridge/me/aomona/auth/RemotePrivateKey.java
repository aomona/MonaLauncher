package me.aomona.auth;

import java.security.PrivateKey;

/** This object contains only an identifier; it is not an RSA private key implementation. */
public final class RemotePrivateKey implements PrivateKey {
    private static final long serialVersionUID = 1L;
    final String id;
    RemotePrivateKey(String id) { this.id = id; }
    @Override public String getAlgorithm() { return "RSA"; }
    @Override public String getFormat() { return null; }
    @Override public byte[] getEncoded() { return null; }
}
