#define _GNU_SOURCE
#include <jni.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/mman.h>
#if defined(__APPLE__)
#include <mach/mach.h>
#include <mach/mach_vm.h>
#elif defined(__linux__)
#include <sys/uio.h>
#else
#error Native memory probe currently supports macOS and Linux only
#endif

static const unsigned char known[] = "MonaNativeMemoryControl";
static int last_error;

static jbyteArray bytes(JNIEnv *env, const unsigned char *data, size_t length) {
    jbyteArray result = (*env)->NewByteArray(env, (jsize)length);
    if (result) (*env)->SetByteArrayRegion(env, result, 0, (jsize)length, (const jbyte *)data);
    return result;
}

static jbyteArray read_process(JNIEnv *env, int pid, uint64_t address, int length) {
    last_error = 0;
    if (pid <= 0 || address == 0 || length <= 0 || length > 32768) {
        last_error = EINVAL; return NULL;
    }
    unsigned char destination[32768];
#if defined(__APPLE__)
    mach_port_t task = MACH_PORT_NULL;
    if (pid == getpid()) task = mach_task_self();
    else {
        kern_return_t status = task_for_pid(mach_task_self(), pid, &task);
        if (status != KERN_SUCCESS) { last_error = status; return NULL; }
    }
    mach_vm_size_t count = 0;
    kern_return_t status = mach_vm_read_overwrite(task, (mach_vm_address_t)address,
        (mach_vm_size_t)length, (mach_vm_address_t)(uintptr_t)destination, &count);
    if (pid != getpid()) mach_port_deallocate(mach_task_self(), task);
    if (count > (mach_vm_size_t)length) { last_error = KERN_INVALID_ARGUMENT; return NULL; }
    if (count > 0) {
        last_error = status;
        return bytes(env, destination, (size_t)count);
    }
    if (status != KERN_SUCCESS || count != (mach_vm_size_t)length) {
        last_error = status == KERN_SUCCESS ? KERN_FAILURE : status; return NULL;
    }
#else
    struct iovec local = { .iov_base = destination, .iov_len = (size_t)length };
    struct iovec remote = { .iov_base = (void *)(uintptr_t)address, .iov_len = (size_t)length };
    ssize_t count = process_vm_readv(pid, &local, 1, &remote, 1, 0);
    if (count > 0 && count <= length) {
        last_error = count == length ? 0 : EIO;
        return bytes(env, destination, (size_t)count);
    }
    if (count != length) { last_error = count < 0 ? errno : EIO; return NULL; }
#endif
    return bytes(env, destination, (size_t)length);
}

JNIEXPORT jbyteArray JNICALL Java_me_aomona_probe_NativeMemoryProbe_readProcess(JNIEnv *env, jclass type, jint pid, jlong address, jint length) {
    (void)type; return read_process(env, pid, (uint64_t)address, length);
}
JNIEXPORT jbyteArray JNICALL Java_me_aomona_probe_NativeMemoryProbe_selfRead(JNIEnv *env, jclass type) {
    (void)type; return read_process(env, getpid(), (uintptr_t)known, sizeof(known) - 1);
}
JNIEXPORT jbyteArray JNICALL Java_me_aomona_probe_NativeMemoryProbe_control(JNIEnv *env, jclass type) {
    (void)type; return bytes(env, known, sizeof(known) - 1);
}
JNIEXPORT jint JNICALL Java_me_aomona_probe_NativeMemoryProbe_errorCode(JNIEnv *env, jclass type) {
    (void)env; (void)type; return last_error;
}

JNIEXPORT jbyteArray JNICALL Java_me_aomona_probe_NativeMemoryProbe_partialSelfRead(JNIEnv *env, jclass type) {
    (void)type;
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) { last_error = EINVAL; return NULL; }
    unsigned char *region = mmap(NULL, (size_t)page * 2, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (region == MAP_FAILED) { last_error = errno; return NULL; }
    memcpy(region + page - 8, "MonaPart", 8);
    if (mprotect(region + page, (size_t)page, PROT_NONE) != 0) {
        last_error = errno; munmap(region, (size_t)page * 2); return NULL;
    }
    jbyteArray result = read_process(env, getpid(), (uintptr_t)(region + page - 8), 16);
    munmap(region, (size_t)page * 2);
    return result;
}
