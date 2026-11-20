# pokshell architecture: Linux first, game session first

[Full Linux-first strategy and language decisions](boot-and-languages.md).

```text
Bootloader + hardware-specific Linux kernel / device tree
    |
existing Debian ARM64 (UTM) or board-supported Linux (handheld)
    |
session/seat management, Mesa, DRM/KMS, input and audio
    |
Rust: pokshelld + game lifecycle / IPC             [started]
    |
Rust: DRM/KMS + Wayland compositor                 [planned]
     +-- native ARM64 Linux game process
     +-- independent system overlay surface
     +-- gamepad/touch focus and multi-screen policy
    |
Rust UI host (technology decision pending)          [planned]
     +-- MoonBit system components via WAMR          [planned]
     +-- Lua optional
     +-- TypeScript only as last resort
```

A Wayland compositor is the intended **system graphical session**, not a traditional desktop environment. We do not create a new kernel or graphical widget toolkit in v0.1.

## Existing implementation

- `rust/`: pure-Rust native-game supervisor, bounded multi-client IPC daemon, live in-memory state-change events and CLI; overlay **state only**; no display backend.
- `src/`: legacy C software framebuffer prototype; useful for testing direct frame access but does **not** implement generic games or a production overlay.

## Intended acceptance gates

1. Real ARM64 Linux userspace run on UTM using a standalone session daemon.
2. Native DRM/KMS-backed Wayland compositor controls focus and game surfaces.
3. Arbitrary native SDL/GLES game continues running while an independently drawn overlay is interactive.
4. A native shell UI renders the game library, with gamepad and touch handling.
5. Optional MoonBit/WAMR components integrate without a JS dependency.
6. Board-specific startup and input/power/graphics evidence for Low/Mid/High reference targets.

Do not call gates 2–6 implemented until tested.
