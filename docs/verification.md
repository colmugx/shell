# Verification status

Evidence is tracked per implementation, per machine and per hardware reference.

| Capability | Evidence | Status |
| --- | --- | --- |
| Legacy C framebuffer alpha on x86-64/ARM64 | GitHub Actions Linux builds/tests | CI verified |
| Rust process supervision, termination grace, event subscription, fragmented IPC | `rust/` unit/integration tests; Actions run 44695342191 | CI passed on x86-64 / ARM64 |
| Rust user session in Debian ARM64 / UTM | User-reported KDE Wayland VM session running pokcomp and SuperTux (2026-11-21); no boot log or native DRM/KMS evidence | Nested session observed by user; boot/native DRM/KMS unverified |
| Bootable Buildroot QEMU image | No QEMU boot log | Unverified |
| Own DRM/KMS and Wayland compositor | Smithay nested Winit/GLES compositor; headless Sway-hosted CI runs actual XDG and Layer Shell clients; native DRM/KMS not implemented | Nested software CI verified; native backend pending |
| Wayland layer-shell diagnostic surface | Rust wl_shm + zwlr_layer_shell_v1 client; Actions 44704676668, x86-64 headless Sway protocol test | Compiled on x86-64 / ARM64; headless protocol handshake passed |
| UTM Wayland graphical session | User-reported acceptance on 2026-11-21: SuperTux is rendered within the nested Smithay host window; QUIT ends the game/session daemon while the separate Smithay host window remains open | Manual UTM acceptance passed for window ownership and process separation only |
| Actual game overlay during arbitrary fullscreen SDL/GLES game | Headless CI verifies XDG/Layer SHM coexistence and unchanged PID. Earlier UTM capture (2026-11-20) showed SuperTux escaping to KDE; following explicit game Wayland socket pinning, user reports SuperTux now inside the Smithay window (2026-11-21). Fullscreen negotiation previously unverified; nested XDG maximize/fullscreen configure and output-resize behavior now implemented with an SDL2 CI regression. the user subsequently reported working fullscreen/resize, Escape-close and game keyboard focus recovery in UTM. GLES acceleration remains unmeasured | Nested SuperTux window ownership and diagnostic input user-accepted (2026-11-21); user later observed system menu attached to KDE host rather than Smithay. Explicit --display nested socket binding added; manual UTM retest pending. GPU acceleration unmeasured |
| MoonBit/WAMR component execution | Not linked | Not implemented |
| Production console UI, controller focus, dual screen | Nested pointer hit testing and click-to-focus implemented; optional panel click-to-close and OnDemand keyboard focus/Escape dismissal (CI protocol/lifecycle and user-reported UTM validation on 2026-11-21). Optional nonexclusive Linux joystick test path; no robust gamepad/touch, production UI or dual-screen policy | New Wayland SHM system menu with fixed Resume/Restart/Stop actions, keyboard/mouse navigation and opt-in /dev/input/js* joystick mapping. CI tests menu focus, IPC actions and joystick event decoding; menu and hardware joystick UTM acceptance pending. No exclusive gamepad isolation or production UI |
| H700, Duo Lite, RP6 physical-device boot / suspend / hardware acceleration | No test reports | Unverified |

**Evidence boundary:** CI verifies native Linux user-space behavior. The 2026-11-21 UTM result is a **user-reported manual acceptance** of nested SuperTux window ownership and independent compositor lifetime, not an independently captured boot log, GPU acceleration measurement, or full-screen/gamepad validation.

## Security/maintenance note

Current Rust daemon uses a user-owned local Unix socket (0600), fixed configured game executable, no shell interpolation, and no TCP API. It is a prototype; it does not yet implement sandboxing, privilege separation, durable signal-independent service restart supervision, durable/replayable events, or a robust cgroup-based descendant process lifecycle. `GAME STOP` uses process-group SIGTERM and a short SIGKILL escalation window. Termination of descendants after the main game process exits independently remains to be addressed. Do not expose this as a privileged system-wide service.

The historical C framebuffer program is not suitable as a production display manager.
