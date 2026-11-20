# Verification status

Evidence is tracked per implementation, per machine and per hardware reference.

| Capability | Evidence | Status |
| --- | --- | --- |
| Legacy C framebuffer alpha on x86-64/ARM64 | GitHub Actions Linux builds/tests | CI verified |
| Rust process supervision, state transitions, command IPC | `rust/` tests + GitHub Actions | CI tested on x86-64 / ARM64 (verify latest run) |
| Rust user session in Debian ARM64 / UTM | No local UTM boot log | Unverified |
| Bootable Buildroot QEMU image | No QEMU boot log | Unverified |
| DRM/KMS and Wayland compositor | Not present | Not implemented |
| Actual game overlay during arbitrary fullscreen SDL/GLES game | Only Rust overlay state, or legacy framebuffer demonstration | Not implemented |
| MoonBit/WAMR component execution | Not linked | Not implemented |
| Production console UI, controller focus, dual screen | Not present | Not implemented |
| H700, Duo Lite, RP6 physical-device boot / suspend / hardware acceleration | No test reports | Unverified |

**CI tests validate native Linux user-space code; they are not evidence of a bootable firmware or actual GPU output.**

## Security/maintenance note

Current Rust daemon uses a user-owned local Unix socket (0600), fixed configured game executable, no shell interpolation, and no TCP API. It is a prototype; it does not yet implement sandboxing, privilege separation, graceful game termination or robust multi-client framing. Do not expose this as a privileged system-wide service.

The historical C framebuffer program is not suitable as a production display manager.
