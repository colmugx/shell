# Verification status

Evidence is tracked per implementation, per machine and per hardware reference.

| Capability | Evidence | Status |
| --- | --- | --- |
| Legacy C framebuffer alpha on x86-64/ARM64 | GitHub Actions Linux builds/tests | CI verified |
| Rust process supervision, termination grace, event subscription, fragmented IPC | `rust/` unit/integration tests; Actions run 44695342191 | CI passed on x86-64 / ARM64 |
| Rust user session in Debian ARM64 / UTM | No local UTM boot log | Unverified |
| Bootable Buildroot QEMU image | No QEMU boot log | Unverified |
| Own DRM/KMS and Wayland compositor | Experimental nested Smithay XDG Wayland server source; no UTM run evidence, no DRM/KMS backend | Source added; runtime unverified |
| Wayland layer-shell diagnostic surface | Rust wl_shm + zwlr_layer_shell_v1 client; Actions 44704676668, x86-64 headless Sway protocol test | Compiled on x86-64 / ARM64; headless protocol handshake passed |
| UTM Wayland graphical session | No direct VM test evidence | Unverified |
| Actual game overlay during arbitrary fullscreen SDL/GLES game | Only Rust overlay state, or legacy framebuffer demonstration | Not implemented |
| MoonBit/WAMR component execution | Not linked | Not implemented |
| Production console UI, controller focus, dual screen | Not present | Not implemented |
| H700, Duo Lite, RP6 physical-device boot / suspend / hardware acceleration | No test reports | Unverified |

**CI tests validate native Linux user-space code; they are not evidence of a bootable firmware or actual GPU output.**

## Security/maintenance note

Current Rust daemon uses a user-owned local Unix socket (0600), fixed configured game executable, no shell interpolation, and no TCP API. It is a prototype; it does not yet implement sandboxing, privilege separation, service-wide SIGINT/SIGTERM shutdown hooks, durable/replayable events, or a robust cgroup-based descendant process lifecycle. `GAME STOP` uses process-group SIGTERM and a short SIGKILL escalation window. Termination of descendants after the main game process exits independently remains to be addressed. Do not expose this as a privileged system-wide service.

The historical C framebuffer program is not suitable as a production display manager.
