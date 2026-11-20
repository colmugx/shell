# pokshell v0.1 language and boot strategy

Updated 2026-11-20. This document records the target architecture and the implemented Rust bootstrap separately.

## Decision

**Start from an existing Linux distribution, implement a dedicated console session, then produce a trimmed system image.** Do not start by writing a kernel or by building a desktop theme on GNOME/KDE.

- **UTM/ARM Mac:** Debian 13 ARM64 under QEMU `virt` for portable userspace testing. This is not a simulator of H700, QCS6125 or SM8550 GPUs.
- **Physical devices:** Start from a working Linux board image / kernel / device tree / boot chain appropriate to the actual handheld. Reuse ROCKNIX and other upstream patches under their original licenses; do not substitute an ARM64 generic rootfs for a complete board port.
- **System image:** Once display/input/suspend and game sessions work, evaluate Arch Linux ARM or Buildroot as a reproducible, stripped-down root filesystem. Kernel choice follows **board support**, not distro branding.
- **Execution model:** start a standalone game-first systemd user/session service, not GNOME/KDE/Xfce and not a browser UI. The Rust runtime is initially terminal/headless; a real Wayland compositor is the next independent engineering milestone.

## Language policy

1. **Rust** is the default for native system services, IPC, process supervision, input routing and a future compositor. Prefer safe Rust and well-maintained typed bindings to system libraries.
2. **MoonBit** is preferred for portable system extension and UI/business logic modules when its backend and target integration are suitable. Candidate deployment: WAMR Core Wasm with an explicitly defined host API. No WIT requirement.
3. **Lua** is acceptable for small scripting/configuration tasks.
4. **TypeScript** is an exceptional dependency only if a mandatory upstream frontend/runtime cannot be used otherwise. No TS/browser/Electron runtime in the v0.1 base.
5. **C** may appear as an ABI boundary, upstream library or historical alpha; do not use it for new application logic by default.

**PocketJS is not a language mandate.** Its current upstream interface/runtime may force a later technical trade-off; verify a viable Rust/MoonBit UI path rather than promising TS-free PocketJS integration before validating it.

## Milestones and acceptance tests

| Gate | Objective | Status |
| --- | --- | --- |
| R0 | Native Rust session daemon, fixed configured game process, IPC, overlay lifecycle state, unit/integration tests | Implemented in `rust/`; GitHub Actions verification required |
| R1 | Run the Rust daemon and native game on ARM64 Debian/UTM without any desktop environment | Not verified in UTM |
| R2 | Real DRM/KMS + GBM/EGL or Vulkan-backed Wayland session and arbitrary native SDL/GLES game; overlay remains interactive and game continues | Not implemented |
| R3 | Polished native console UI, game library, controller/touch focus; MoonBit component executes via WAMR | Not implemented |
| R4 | Board-specific startup on H700, Duo Lite and RP6 with driver/thermal/power checks | Not implemented |
| R5 | Reproducible image and performance budgets on the three hardware baselines | Not implemented |

The `OVERLAY OPEN/CLOSE` IPC in R0 **changes state only**. It does not draw a layer over a running game. Nothing in R0 claims to be a full OS.

## Why keep the earlier C alpha?

The C `src/` code remains an historical framebuffer experiment and headless smoke-test baseline, not the future architecture. The Rust path is additive until it independently covers the required runtime behavior, at which point the C framebuffer alpha can move to `experiments/` and be removed from default images.

## Development commands

On a Linux machine with Rust/Cargo and Python 3:

```sh
cd rust
cargo test --workspace --locked
cargo build --workspace --locked
python3 tests/ipc_integration.py
```

Run a configured test game as your ordinary user:

```sh
cd rust
cargo run --bin pokshelld -- --game /bin/sleep --game-arg 120
# In a second terminal:
cargo run --bin pokctl -- STATUS
cargo run --bin pokctl -- GAME START
cargo run --bin pokctl -- OVERLAY OPEN
cargo run --bin pokctl -- GAME STOP
cargo run --bin pokctl -- QUIT
```

Sockets live at `$XDG_RUNTIME_DIR/pokshell.sock` if set, else a local `./pokshell.sock` for development. The CLI/server do not support remote TCP access. The fixed configured executable is never interpolated through a shell.

## Upstream investigation targets

- Smithay: https://github.com/Smithay/smithay (Rust components for a Wayland compositor; **not** a ready-made console session)
- Mesa / Linux DRM KMS: https://docs.kernel.org/gpu/drm-kms.html
- WAMR: https://github.com/bytecodealliance/wasm-micro-runtime
- MoonBit language docs: https://docs.moonbitlang.com/
