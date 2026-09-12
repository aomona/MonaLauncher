import ctypes, errno, json, os, pathlib, platform, socket, subprocess, sys
root = pathlib.Path(sys.argv[1])
game = root / 'instances/one/game'
temp = root / 'instances/one/launch/tmp'

def attempt(action):
    try:
        action()
        return True
    except (OSError, subprocess.CalledProcessError):
        return False

def connect(family, address):
    with socket.socket(family, socket.SOCK_STREAM) as s:
        s.settimeout(1)
        s.connect(address)

def create(family):
    with socket.socket(family, socket.SOCK_STREAM):
        pass

libc = ctypes.CDLL(None, use_errno=True)
clone3 = 435
result = {
    'host_read': attempt(lambda: (root / 'host/secret').read_bytes()),
    'other_read': attempt(lambda: (root / 'instances/two/secret').read_bytes()),
    'symlink_read': attempt(lambda: (game / 'escape').read_bytes()),
    'shared_read': attempt(lambda: (root / 'libraries/shared').read_bytes()),
    'shared_write': attempt(lambda: (root / 'libraries/probe-write').write_text('probe')),
    'game_write': attempt(lambda: (game / 'probe-write').write_text('probe')),
    'temp_write': attempt(lambda: (temp / 'probe-write').write_text('probe')),
    'child_host_read': attempt(lambda: subprocess.run(['/usr/bin/python3', '-c', 'import pathlib,sys; pathlib.Path(sys.argv[1]).read_bytes()', str(root / 'host/secret')], check=True, stderr=subprocess.DEVNULL)),
    'tcp': attempt(lambda: connect(socket.AF_INET, ('127.0.0.1', int(sys.argv[2])))),
    'host_socket': attempt(lambda: connect(socket.AF_UNIX, str(root / 'host/service'))),
    'inet_socket': attempt(lambda: create(socket.AF_INET)),
    'inet6_socket': attempt(lambda: create(socket.AF_INET6)),
    'unix_socket': attempt(lambda: create(socket.AF_UNIX)),
    'unshare': libc.unshare(0) == 0,
    'ptrace': libc.ptrace(2, 999999, 0, 0) != -1 or ctypes.get_errno() != errno.EPERM,
}
# PTRACE_PEEKDATA targets a nonexistent PID; it never attaches to a host process.
result['clone3_fallback'] = libc.syscall(clone3, 0, 0) == -1 and ctypes.get_errno() == errno.ENOSYS
status = pathlib.Path('/proc/self/status').read_text()
result['seccomp'] = 'Seccomp:\t2' in status
result['no_new_privs'] = 'NoNewPrivs:\t1' in status
if len(sys.argv) > 3 and sys.argv[3] == 'wayland':
    runtime = pathlib.Path(os.environ['XDG_RUNTIME_DIR'])
    display = pathlib.Path(os.environ['WAYLAND_DISPLAY'])
    result.update({
        'wayland_connect': attempt(lambda: connect(socket.AF_UNIX, str(runtime / display))),
        'x11_path': attempt(lambda: connect(socket.AF_UNIX, '/tmp/.X11-unix/X0')),
        'x11_abstract': attempt(lambda: connect(socket.AF_UNIX, '\0/tmp/.X11-unix/X0')),
        'display_env': 'DISPLAY' in os.environ,
        'xauthority_env': 'XAUTHORITY' in os.environ,
        'session_bus': attempt(lambda: connect(socket.AF_UNIX, '/run/user/1000/bus')),
        'sys_network': pathlib.Path('/sys/class/net').exists(),
        'drm_primary': any(pathlib.Path('/dev/dri').glob('card*')),
        'input_devices': pathlib.Path('/dev/input').exists(),
        'gpu_config': any(pathlib.Path('/sys/class/drm').glob('*/device/config')),

    })
    mounts = pathlib.Path('/proc/self/mountinfo').read_text().splitlines()
    vendors = list(pathlib.Path('/sys/class/drm').glob('renderD*/device/vendor'))
    result['gpu_vendor_read'] = bool(vendors) and all(attempt(p.read_bytes) for p in vendors)
    result['gpu_vendor_readonly_mount'] = bool(vendors) and all(
        any(line.split()[4] == str(p.resolve()) and 'ro' in line.split()[5].split(',') for line in mounts)
        for p in vendors)
print(json.dumps(result), flush=True)
