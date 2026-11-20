# Verification status

Evidence is tracked per implementation, per machine and per hardware reference.

| Capability | Evidence | Status |
| --- | --- | --- |
| Legacy C framebuffer alpha on x86-64/ARM64 | GitHub Actions Linux builds/tests | CI verified |
| Rust process supervision, termination grace, event subscription, fragmented IPC | `rust/` unit/integration tests; Actions run 44695342191 | CI passed on x86-64 / ARM64 |
| Rust user session in Debian ARM64 / UTM | No local UTM boot log | Unverified |
| Bootable Buildroot QEMU image | No QEMU boot log | Unverified |
| Own DRM/KMS and Wayland compositor | Smithay nested Winit/GLES compositor; headless Sway-hosted CI runs actual XDG and Layer Shell clients; native DRM/KMS not implemented | Nested software CI verified; native backend pending |
| Wayland layer-shell diagnostic surface | Rust wl_shm + zwlr_layer_shell_v1 client; Actions 44704676668, x86-64 headless Sway protocol test | Compiled on x86-64 / ARM64; headless protocol handshake passed |
| UTM Wayland graphical session | No direct VM test evidence | Unverified |
| Actual game overlay during arbitrary fullscreen SDL/GLES game | Headless CI verifies weston-simple-shm XDG render elements, visible/transparent Layer SHM commits and unchanged game PID; fullscreen SDL/GLES and UTM remain untested | Native SHM coexistence verified in headless CI; fullscreen/GLES/UTM unverified |
| MoonBit/WAMR component execution | Not linked | Not implemented |
| Production console UI, controller focus, dual screen | Not present | Not implemented |
| H700, Duo Lite, RP6 physical-device boot / suspend / hardware acceleration | No test reports | Unverified |

**CI tests validate native Linux user-space code; they are not evidence of a bootable firmware or actual GPU output.**

## Security/maintenance note

Current Rust daemon uses a user-owned local Unix socket (0600), fixed configured game executable, no shell interpolation, and no TCP API. It is a prototype; it does not yet implement sandboxing, privilege separation, durable signal-independent service restart supervision, durable/replayable events, or a robust cgroup-based descendant process lifecycle. `GAME STOP` uses process-group SIGTERM and a short SIGKILL escalation window. Termination of descendants after the main game process exits independently remains to be addressed. Do not expose this as a privileged system-wide service.

The historical C framebuffer program is not suitable as a production display manager.
