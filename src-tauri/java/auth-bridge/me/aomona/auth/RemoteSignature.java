package me.aomona.auth;

import java.io.ByteArrayOutputStream;
import java.security.*;

/** Selected by JCA only for opaque keys; ordinary RSA operations keep their normal provider. */
@SuppressWarnings("deprecation")
public final class RemoteSignature extends SignatureSpi {
    private RemotePrivateKey key;
    private final ByteArrayOutputStream message = new ByteArrayOutputStream();
    private boolean failed;

    @Override protected void engineInitSign(PrivateKey key) throws InvalidKeyException {
        this.key = null; message.reset(); failed = false;
        if (!(key instanceof RemotePrivateKey remote)) throw new InvalidKeyException("A broker key is required");
        this.key = remote;
    }
    @Override protected void engineInitVerify(PublicKey key) throws InvalidKeyException {
        throw new InvalidKeyException("Use a standard provider to verify chat signatures");
    }
    @Override protected void engineUpdate(byte value) throws SignatureException {
        engineUpdate(new byte[] { value }, 0, 1);
    }
    @Override protected void engineUpdate(byte[] bytes, int offset, int count) throws SignatureException {
        if (key == null || failed || offset < 0 || count < 0 || offset > bytes.length - count
            || count > 6208 - message.size()) {
            failed = true; throw new SignatureException("Chat signature input invalid");
        }
        message.write(bytes, offset, count);
    }
    @Override protected byte[] engineSign() throws SignatureException {
        if (key == null || failed) throw new SignatureException("Chat signer not ready");
        byte[] bytes = message.toByteArray(); message.reset();
        return AuthBridge.signChat(key.id, bytes);
    }
    @Override protected boolean engineVerify(byte[] signature) throws SignatureException {
        throw new SignatureException("Verification is not a broker operation");
    }
    @Override protected void engineSetParameter(String name, Object value) {
        throw new InvalidParameterException("No configurable signature parameters");
    }
    @Override protected Object engineGetParameter(String name) {
        throw new InvalidParameterException("No configurable signature parameters");
    }
}
