// Tests only fixture networking and service lookup; never reads the user's clipboard or microphone.
#include <mach/mach.h>
#include <servers/bootstrap.h>
#include <sys/socket.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
extern int sandbox_check(pid_t, const char *, int, ...);
static int service(const char *name) {
    mach_port_t port = MACH_PORT_NULL;
    int ok = bootstrap_look_up(bootstrap_port, name, &port) == KERN_SUCCESS;
    if (ok) mach_port_deallocate(mach_task_self(), port);
    return ok;
}
int main(int argc, char **argv) {
    if (argc != 3) return 2;
    alarm(5);
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address = {0};
    address.sin_family = AF_INET;
    address.sin_port = htons(atoi(argv[1]));
    inet_pton(AF_INET, "127.0.0.1", &address.sin_addr);
    int network = fd >= 0 && connect(fd, (struct sockaddr *)&address, sizeof(address)) == 0;
    if (fd >= 0) close(fd);
    char fixture[4096];
    snprintf(fixture, sizeof(fixture), "%s/mona-permission-probe-%d", argv[2], getpid());
    FILE *cache = fopen(fixture, "wx");
    int graphics = cache != NULL;
    if (cache) { fputs("fixture", cache); fclose(cache); unlink(fixture); }
    printf("{\"network\":%d,\"audio\":%d,\"clipboard\":%d,\"fullscreen\":%d,\"graphics_cache\":%d,\"microphone_policy\":%d}\n", network,
        service("com.apple.audio.audiohald"), service("com.apple.pasteboard.1"),
        service("com.apple.dock.fullscreen"),
        graphics,
        sandbox_check(getpid(), "device-microphone", 0) == 0);
    return 0;
}
