# pokshell system architecture

**Canonical product and milestone contract:** [ROADMAP.md](../ROADMAP.md).
**Codex parallel development:** [agent work packages](agent-workstreams.md).
This document records component boundaries; it does not override the roadmap.

```text
Linux kernel + device tree + open GPU/input/audio drivers + minimal userspace
    |
Native Rust system services (seat, display, game lifecycle, power, IPC)
    +-- Smithay Wayland compositor
    |    +-- DRM/KMS connector/CRTC/plane and vblank (P1 in progress)
    |    +-- GBM/EGL/GLES or capable Vulkan import/render path
    |    +-- game XDG/layer-shell surfaces, overlays, input focus
    +-- Game runtime (native Linux, SDL/GLES/Vulkan, Proton as appropriate)
    +-- Emulator Core Host (P3 Libretro-like core adapters, not full frontends)
    +-- PocketJS system host (P2)
         +-- HOME, Library, Settings, Overlay, Plugins
         +-- native pixel/UI pipeline; normal unprivileged application code
```

The *sole production shell* is PocketJS. Its upstream JavaScript/TypeScript
application contract and native core are intentional; React/Electron/browser
or KDE are not runtime dependencies. Rust owns privileged session/device
operations and latency-sensitive paths. Portable MoonBit/WAMR and Lua
components may be added where justified and measured; they are not a
prerequisite to a working PocketJS UI.

## Current implementation boundaries

- `rust/`: native game-process supervisor `pokshelld`, `pokctl` IPC,
  `poksession` nested developer harness, and read-only `pokdrm`.
- `rust/compositor`: nested Smithay Winit Wayland server used to prove
  SuperTux+menu input coexistence in KDE/UTM; **not** the target display.
- `graphics/wayland-layer`: Rust SHM game menu diagnostic client. This is
  replaced/hosted by PocketJS once the UI contract is integrated.
- `rust/native`: independent libseat/udev/libinput/DRM/GBM/EGL/GLES
  initialization, KMS route selection and seat generation guard. It
  **does not yet modeset, page flip or render a Wayland game natively**.
- `src/`: disposable C framebuffer experiment; not product code.

## Engineering gates

P1: native TTY seat + atomic KMS scanout + game and overlay on same output;
input isolation, hotplug, pause/resume, safe release.

P2: reproducible PocketJS native UI build/host, system RPC, input integration,
Home/Library/Settings/Overlay, performance evidence.

P3: independent emulator cores, system-owned video/audio/input/saves, safe
fallback for incompatible cores.

P4: device-specific slim Linux image, updates/recovery, tested GPU/power and
suspend/resume, measured battery performance.

See [native Linux acceptance](native-linux.md) and
[verification evidence](verification.md). Passing a nested Wayland test
does not satisfy native scanout, and compiling EGL does not establish
hardware rendering.
