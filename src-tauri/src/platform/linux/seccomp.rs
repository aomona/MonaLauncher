//! Native-ABI filter, installed by bubblewrap before executing the JVM.
use std::io;

const LOAD: u16 = 0x20;
const JEQ: u16 = 0x15;
const JSET: u16 = 0x45;
const RET: u16 = 0x06;
const ALLOW: u32 = 0x7fff0000;
const KILL: u32 = 0x80000000;
const ERRNO: u32 = 0x00050000;

pub fn program() -> io::Result<Vec<u8>> {
    let arch = if cfg!(target_arch = "x86_64") {
        0xc000003e
    } else if cfg!(target_arch = "aarch64") {
        0xc00000b7
    } else {
        return Err(io::Error::other(
            "Linux sandbox supports x86_64 and aarch64 only",
        ));
    };
    let mut filter = Vec::new();
    let mut emit = |code: u16, jt: u8, jf: u8, value: u32| {
        filter.extend(code.to_ne_bytes());
        filter.extend([jt, jf]);
        filter.extend(value.to_ne_bytes());
    };
    emit(LOAD, 0, 0, 4); // seccomp_data.arch
    emit(JEQ, 1, 0, arch);
    emit(RET, 0, 0, KILL);
    emit(LOAD, 0, 0, 0); // seccomp_data.nr
                         // Reject x32 as well as foreign audit architectures.
    emit(JSET, 0, 1, 0x40000000);
    emit(RET, 0, 0, KILL);
    for syscall in [
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_userfaultfd,
        libc::SYS_open_by_handle_at,
        libc::SYS_move_mount,
        libc::SYS_fsopen,
        libc::SYS_fsconfig,
        libc::SYS_fsmount,
        libc::SYS_fspick,
        libc::SYS_mount_setattr,
        libc::SYS_reboot,
        libc::SYS_kexec_load,
    ] {
        emit(JEQ, 0, 1, syscall as u32);
        emit(RET, 0, 0, ERRNO | libc::EPERM as u32);
    }
    // glibc falls back to clone when clone3 is unavailable. Its pointed-to arguments
    // cannot be safely inspected by classic BPF.
    emit(JEQ, 0, 1, libc::SYS_clone3 as u32);
    emit(RET, 0, 0, ERRNO | libc::ENOSYS as u32);
    emit(JEQ, 0, 3, libc::SYS_clone as u32);
    emit(LOAD, 0, 0, 16); // args[0], flags on supported little-endian ABIs
    emit(
        JSET,
        0,
        1,
        (libc::CLONE_NEWUSER
            | libc::CLONE_NEWNS
            | libc::CLONE_NEWNET
            | libc::CLONE_NEWPID
            | libc::CLONE_NEWIPC
            | libc::CLONE_NEWUTS
            | libc::CLONE_NEWCGROUP) as u32,
    );
    emit(RET, 0, 0, ERRNO | libc::EPERM as u32);
    emit(LOAD, 0, 0, 0);
    for syscall in [libc::SYS_socket, libc::SYS_socketpair] {
        emit(JEQ, 0, 3, syscall as u32);
        emit(LOAD, 0, 0, 16);
        emit(JEQ, 1, 0, libc::AF_UNIX as u32);
        emit(RET, 0, 0, ERRNO | libc::EAFNOSUPPORT as u32);
        emit(LOAD, 0, 0, 0);
    }
    emit(JEQ, 0, 5, libc::SYS_ioctl as u32);
    emit(LOAD, 0, 0, 24); // args[1], request
    emit(JEQ, 0, 1, libc::TIOCSTI as u32);
    emit(RET, 0, 0, ERRNO | libc::EPERM as u32);
    emit(JEQ, 0, 1, 0x541c); // TIOCLINUX
    emit(RET, 0, 0, ERRNO | libc::EPERM as u32);
    emit(RET, 0, 0, ALLOW);
    Ok(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn evaluate(nr: u32, arch: u32, arg0: u32, arg1: u32) -> u32 {
        let code = program().unwrap();
        let mut pc = 0;
        let mut accumulator = 0;
        for _ in 0..code.len() / 8 {
            let insn = &code[pc * 8..pc * 8 + 8];
            let op = u16::from_ne_bytes(insn[..2].try_into().unwrap());
            let k = u32::from_ne_bytes(insn[4..].try_into().unwrap());
            match op {
                LOAD => {
                    accumulator = match k {
                        0 => nr,
                        4 => arch,
                        16 => arg0,
                        24 => arg1,
                        _ => panic!("unexpected input offset"),
                    }
                }
                JEQ | JSET => {
                    let matched = if op == JEQ {
                        accumulator == k
                    } else {
                        accumulator & k != 0
                    };
                    pc += if matched { insn[2] } else { insn[3] } as usize;
                }
                RET => return k,
                _ => panic!("unexpected filter instruction"),
            }
            pc += 1;
        }
        panic!("filter did not return")
    }
    #[test]
    fn filter_allows_jvm_threads_and_unix_ipc_but_denies_escape_primitives() {
        let arch = if cfg!(target_arch = "aarch64") {
            0xc00000b7
        } else {
            0xc000003e
        };
        for (nr, arg0, arg1, expected) in [
            (libc::SYS_getpid, 0, 0, ALLOW),
            (
                libc::SYS_clone,
                (libc::CLONE_VM | libc::CLONE_THREAD) as u32,
                0,
                ALLOW,
            ),
            (
                libc::SYS_clone,
                libc::CLONE_NEWUSER as u32,
                0,
                ERRNO | libc::EPERM as u32,
            ),
            (libc::SYS_socket, libc::AF_UNIX as u32, 0, ALLOW),
            (
                libc::SYS_socket,
                libc::AF_INET6 as u32,
                0,
                ERRNO | libc::EAFNOSUPPORT as u32,
            ),
            (
                libc::SYS_socketpair,
                libc::AF_NETLINK as u32,
                0,
                ERRNO | libc::EAFNOSUPPORT as u32,
            ),
            (
                libc::SYS_ioctl,
                0,
                libc::TIOCSTI as u32,
                ERRNO | libc::EPERM as u32,
            ),
            (libc::SYS_ioctl, 0, libc::TIOCGWINSZ as u32, ALLOW),
            (libc::SYS_clone3, 0, 0, ERRNO | libc::ENOSYS as u32),
            (libc::SYS_mount, 0, 0, ERRNO | libc::EPERM as u32),
        ] {
            assert_eq!(
                evaluate(nr as u32, arch, arg0, arg1),
                expected,
                "syscall {nr}"
            );
        }
        assert_eq!(evaluate(libc::SYS_getpid as u32, 0, 0, 0), KILL);
        assert_eq!(evaluate(0x40000027, arch, 0, 0), KILL);
    }
}
