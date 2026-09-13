#include <jni.h>
#include <stdint.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <unistd.h>
#endif

static void fail(JNIEnv *env) {
    jclass exception = (*env)->FindClass(env, "java/io/IOException");
    if (exception) (*env)->ThrowNew(env, exception, "Authentication IPC unavailable");
}

JNIEXPORT void JNICALL Java_me_aomona_auth_NativeIO_prepare(JNIEnv *env, jclass type, jlong handle) {
    (void) type;
#ifdef _WIN32
    if (!SetHandleInformation((HANDLE)(intptr_t)handle, HANDLE_FLAG_INHERIT, 0)) fail(env);
#else
    if (handle < 0 || handle > INT32_MAX) { fail(env); return; }
    int flags = fcntl((int)handle, F_GETFL);
    if (flags < 0 || fcntl((int)handle, F_SETFD, FD_CLOEXEC) < 0
        || fcntl((int)handle, F_SETFL, flags | O_NONBLOCK) < 0) fail(env);
#endif
}

static jint transfer(JNIEnv *env, jlong handle, jbyteArray array, jint offset, jint count, int writing) {
    if (!array || offset < 0 || count < 0 || count > 262144 || offset > (*env)->GetArrayLength(env, array) - count) {
        fail(env); return -1;
    }
    jbyte *bytes = (*env)->GetByteArrayElements(env, array, NULL);
    if (!bytes) return -1;
    int result = -1;
#ifdef _WIN32
    DWORD completed = 0;
    BOOL ok = writing ? WriteFile((HANDLE)(intptr_t)handle, bytes + offset, (DWORD)count, &completed, NULL)
                      : ReadFile((HANDLE)(intptr_t)handle, bytes + offset, (DWORD)count, &completed, NULL);
    if (ok) result = (int)completed;
#else
    struct pollfd fd = { .fd = (int)handle, .events = writing ? POLLOUT : POLLIN, .revents = 0 };
    int ready;
    do { ready = poll(&fd, 1, 35000); } while (ready < 0 && errno == EINTR);
    if (ready > 0) {
        ssize_t size;
        do { size = writing ? write((int)handle, bytes + offset, (size_t)count)
                            : read((int)handle, bytes + offset, (size_t)count); } while (size < 0 && errno == EINTR);
        result = (int)size;
    }
#endif
    (*env)->ReleaseByteArrayElements(env, array, bytes, writing ? JNI_ABORT : 0);
    if (result <= 0) { fail(env); return -1; }
    return result;
}

JNIEXPORT jint JNICALL Java_me_aomona_auth_NativeIO_read(JNIEnv *env, jclass type, jlong handle, jbyteArray bytes, jint offset, jint count) {
    (void) type; return transfer(env, handle, bytes, offset, count, 0);
}
JNIEXPORT jint JNICALL Java_me_aomona_auth_NativeIO_write(JNIEnv *env, jclass type, jlong handle, jbyteArray bytes, jint offset, jint count) {
    (void) type; return transfer(env, handle, bytes, offset, count, 1);
}
