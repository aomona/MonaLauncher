# Linux launcher integration validation

These probes call the production `sandbox::SandboxPolicy`, Linux bubblewrap backend, instance installer, permission persistence, and launcher. They are separate from the older `tools/sandbox-lab` prototype.

## Desktop dependencies and scope

- Linux x86_64 or ARM64, bubblewrap 0.9+, unprivileged user namespaces and seccomp.
- Local X11 or XWayland, a regular Xauthority file, and a local PulseAudio-compatible server (including PipeWire PulseAudio).
- eSpeak NG for the trusted narrator broker. The current worker uses its default voice; Japanese voice quality is not verified.
- Tauri build dependencies on Ubuntu: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libssl-dev build-essential pkg-config`, Rust, and a JDK providing `javac`/`jar`.
- Probe helpers: `x11-utils pulseaudio-utils`; screenshot inspection additionally uses `scrot`.

The launcher does not disable the host's namespace restrictions. On Ubuntu with AppArmor's restricted unprivileged namespaces, install the root-owned application at `/usr/bin/monalauncher` and register `packaging/linux/monalauncher.apparmor`. This profile grants namespace eligibility, not confinement. Distribution packaging still needs to arrange installation and upgrades of this file. Do not apply the profile to a user-writable binary or turn off AppArmor globally.

The dedicated UTM test guest uses root-owned binaries under `/usr/local/libexec/monalauncher-probes/` and the test-only `monalauncher-probes.apparmor` alongside them. Run all probes as `mona`, not root. The VM UUID is recorded in the older [UTM lab report](../sandbox-lab/results/2026-09-12-linux-utm.md). Use the existing `exec.applescript` and `utmctl file` helpers; do not attach serial input or capture the host keyboard/mouse.

## Checks

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --lib \
  --bin linux_sandbox_probe --bin linux_minecraft_smoke --locked -- -D warnings
cargo build -j 1 --manifest-path src-tauri/Cargo.toml --locked \
  --features tauri/custom-protocol \
  --bin monalauncher --bin linux_sandbox_probe --bin linux_minecraft_smoke
```

Run `pnpm build` before the standalone application build so the custom protocol embeds current frontend assets. Install the built probes into the root-owned path, then load the test AppArmor profile. `linux_sandbox_probe` compares an unrestricted positive control with sandboxed read-write and read-only game permissions. It checks file boundaries, symlink and child access, direct TCP/IPv4/IPv6 denial, hidden host Unix sockets, seccomp/no-new-privileges, survival after the caller thread exits, and shutdown of detached descendants. A bubblewrap setup error is a failed test, not an access-denial PASS.

The production backend keeps a dedicated spawning thread alive until its process owner is dropped. Linux parent-death signals follow the spawning thread: spawning directly from Tauri's temporary blocking worker caused the game to die when that worker retired. Also exercise Play in the native application for at least 25 seconds; a CLI-only smoke test does not reproduce that failure.

The UTM guest needs a virtual sound device (this validation uses `intel-hda`). Check that PulseAudio reports an actual device sink instead of Dummy Output. Output-monitor samples establish generated audio, not human acoustic confirmation; do not record a microphone.

```sh
/usr/local/libexec/monalauncher-probes/linux_sandbox_probe

DISPLAY=:0 XAUTHORITY=/run/user/1000/gdm/Xauthority \
XDG_RUNTIME_DIR=/run/user/1000 LIBGL_ALWAYS_SOFTWARE=1 \
MONALAUNCHER_EXPECT_NARRATOR=1 \
/usr/local/libexec/monalauncher-probes/linux_minecraft_smoke \
  /home/mona/.local/share/me.aomona.monalauncher/minecraft 60 26.2 narrator-on
```

Repeat with `narrator-off`. Each run persists that permission through the production API and starts the game from those saved values. The disabled case must not create a broker token or emit narrator requests. The enabled case must complete an eSpeak process. The CLI observes a viewable Minecraft window, sound-engine initialization, and a Java PulseAudio stream; these are not a user's acoustic confirmation. It kills its owned process after the observation interval. Read-only game access is tested by the boundary probe: Minecraft may fail to start or save when that permission is disabled.

On ARM64, the 26.2 metadata's x64-only LWJGL 3.4.1 native jars are replaced with explicitly pinned upstream ARM64 artifacts in `src-tauri/src/minecraft/linux-arm64-natives.json`. Installer, launcher and diagnosis apply the same in-memory mapping. The original Mojang metadata remains unchanged on disk. Jar size and SHA-1 are verified by the existing downloader; Maven URLs are allowed only for those exact checked-in pins. Older/unpinned LWJGL and legacy classifiers fail explicitly rather than selecting unverified replacements. This is not blanket ARM64 compatibility for Minecraft or arbitrary Mods.

The Linux JRE's legal-document symlinks are materialized as regular copies within the archive's legal tree. Links elsewhere, escaping references, missing/cyclic aliases and non-file targets are rejected. No symlink is created during extraction.

## Boundaries

X11 and PulseAudio are compatibility service grants. They permit more than drawing one window and playing sound: other X11 clients and recording/server operations may be reachable. No host D-Bus, full home/runtime directory, input devices, or ALSA devices are mounted. Direct network syscalls are denied, but this does not sanitize every operation offered by an exposed desktop service. See [the shared policy](../../docs/sandbox-policy.md).

The initial UTM validation used software rendering (`llvmpipe`); do not treat that run as GPU-driver coverage. Native Wayland, Linux Microsoft credential storage/sign-in, arbitrary Minecraft/Mod versions, Windows runtime behavior, and production package installation are separate work.

The subsequent [UTM 4.7.5 GPU comparison](utm-4.7.5-gpu-2026-09-12.md) detected accelerated Apple M4 Pro rendering, but both ANGLE backends exposed only OpenGL 2.1 to this guest. Minecraft 26.2 requires 3.3 and did not launch through that GPU path; an unrestricted core-context probe failed too.

[UTM 5.0.5 Beta with Apple Core OpenGL](utm-5.0.5-gpu-2026-09-12.md) subsequently exposed OpenGL 4.1 and passed the GPU context/readback and sandboxed Minecraft smoke tests. In that VM, launch the native UI with `WEBKIT_DISABLE_DMABUF_RENDERER=1` to avoid missing text. Do not set `LIBGL_ALWAYS_SOFTWARE=1` for this GPU test. This covers the virtual M4 Pro graphics path, not physical Linux GPU drivers.

## Upstream references

- [bubblewrap options](https://github.com/containers/bubblewrap/blob/main/bwrap.xml): namespace lifetime, seccomp FD and nested user-namespace controls.
- [Flatpak PulseAudio integration](https://github.com/flatpak/flatpak/blob/main/common/flatpak-run-pulseaudio.c): local socket mount and no-shared-memory client configuration. We do not copy its ALSA device grant.
- [LWJGL supported platforms](https://github.com/LWJGL/lwjgl3/blob/master/README.md): ARM64 natives. Pinned artifacts come from Maven Central and were checked against its published SHA-1 files.
