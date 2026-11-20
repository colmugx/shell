# System architecture (target, not a claim of completed implementation)

```text
Bootloader / per-board firmware
    |
Linux kernel + DT + Mesa DRM/KMS + input/audio/network drivers
    |
minimal rootfs / init
    |
console session (native, privilege-separated)
    +-- full-screen native ARM64 game session
    +-- lightweight compositor and overlay surface [planned]
    +-- PocketJS shell [planned]
    +-- input focus/router and OSK [planned]
    +-- native system services and game supervision
    +-- WAMR Core Wasm workers for system/overlay components [planned]
```

## Priorities

1. Games are ordinary **native Linux processes**; not WebAssembly applications.
2. The UI is a **console Shell**, not a desktop or browser. PocketJS is a planned implementation detail, not yet working on Linux in this repository.
3. System components and sidebar logic may run in **WAMR**. No WIT or Component Model requirement in phase one.
4. Avoid synchronous Wasm execution on the game/compositor render thread. Event-driven modules should be suspended when not needed.
5. Overlay must coexist with arbitrary fullscreen native games. A shared-memory demo framebuffer is **not** proof of this behavior.
6. A game process failure must not kill the system session; Shell failure must not kill the game process.
7. Device capabilities (dual screen, touch, GPU, keyboard, sleep) are optional and discovered per device.

## Current alpha internals

- Native Linux C program; Linux-specific `/dev/fb0` plus evdev.
- Single-process software blitter for menu and overlay.
- Independent `pocket-game` child using a shared `memfd` framebuffer and simple atomic input state.
- Local Unix socket controls via `pocketctl`.
- No production compositor, no WAMR link, no PocketJS, no arbitrary-game graphics IPC.

The alpha renderer is **disposable**. Do not develop it into a proprietary desktop/UI framework.

## Exit criteria for the next milestone

On ARM64 Linux in QEMU/UTM, a real native SDL/GLES game runs under a DRM/KMS-backed compositor, with an independently drawn interactive overlay that can be opened and closed without stopping the game. The PocketJS host and WAMR components must then be integrated against this architecture.
