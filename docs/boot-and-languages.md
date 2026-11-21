# pokshell boot strategy and language policy

See [ROADMAP.md](../ROADMAP.md) for the authoritative product roadmap and
[architecture](architecture.md) for current component boundaries.

## Boot and platform strategy

Start from a working upstream Linux kernel/driver stack and a conventional
userspace while P1 is being developed. Debian ARM64 in UTM is a reproducible
**nested integration harness**, not an emulation of target Mali/Adreno GPUs.
On physical handhelds use a device-specific working boot chain, kernel,
device tree and firmware with matching DRM/Mesa/input/power drivers.

Only after native graphics/seat/game lifecycle works should we produce a
reproducible slim Linux image using a tool such as Buildroot or Yocto.
Minimize *measured* RAM, CPU wakeups, idle power and background processes,
while preserving safety, driver support, networking/storage/audio and recovery.

No production dependency on KDE, GNOME, Xfce or a browser is allowed.
`poksession`/Winit/KDE remain debug tools, not the intended boot target.

## Language and runtime policy

1. **Rust** for Linux session, DRM/KMS/Smithay, game-process supervision,
   input/audio/power, permissioned system RPC and emulator integration.
2. **PocketJS** is the production shell UI, using its **actual upstream
   JavaScript/TypeScript component contract**, QuickJS/native Rust host
   infrastructure and supported rendering APIs. The UI is not a separate
   Electron/browser application. Do not promise TS-free PocketJS.
3. **MoonBit/WAMR** where proven useful for portable or isolated optional
   components; **Lua** for tiny configuration/scripts when justified.
4. **C/C++** only where demanded by kernel/graphics/emulator ABI boundaries or
   existing upstream cores. Preserve third-party licenses.

Project goals are performance, efficiency, compatibility and power budgets,
not minimizing source-language count. Avoid adding extra processes where they
duplicate an existing component, but do not merge untrusted cores into the
privileged system process without isolation evidence.

## Acceptance and reproducibility

Follow `ROADMAP.md` P1–P4 gates. The required progression is:
source → compiled on x86-64/ARM64 → headless protocol → UTM manual →
real seat/DRM scanout → actual handheld/performance/power acceptance.

A DRM-capable Linux device must be accessed as an authorized **non-root**
seat user on a separate TTY, not via root or a KDE session takeover.
The legacy C framebuffer experiment remains a CI baseline only until
deliberately retired.

## Upstream interfaces

- PocketJS: https://github.com/pocket-nexus/pocketjs
- Smithay: https://github.com/Smithay/smithay
- Linux DRM/KMS: https://docs.kernel.org/gpu/drm-kms.html
- Libretro core interface: https://docs.libretro.com/development/cores/
- Mesa: https://mesa3d.org/
- WAMR (optional): https://github.com/bytecodealliance/wasm-micro-runtime
