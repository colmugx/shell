# Native Linux graphics backend (TTY) — device acquisition milestone

The **production target** is a direct Linux Wayland compositor that owns a
DRM/KMS output through libseat/logind or seatd, uses GBM/EGL for rendering,
and receives libinput events from the active seat. The existing
`rust/compositor` Winit window remains the non-destructive KDE/UTM test
backend. The C framebuffer prototype is not the target graphics stack.

## Current implementation

- `rust/apps/pokdrm`: dependency-free, **read-only** Linux DRM device
  and connector discovery using the kernel DRM_IOCTL_MODE_GETRESOURCES ABI.
  Safe to run from regular KDE Wayland Konsole; never requests DRM master.
- `rust/native` (`poknative`): **separate Smithay backend bring-up** using
  libseat for a seat-owned DRM file descriptor, kernel KMS resource enumeration,
  and GBM device initialization. **No DRM modeset, buffers or Wayland compositor
  are attached yet**. It refuses to run inside KDE by default. It is intended
  for a separate native Linux TTY, not a nested KDE window.

The implementation deliberately leaves KMS plane/CRTC state unchanged until
mode selection, vt suspend/resume and hardware-specific recovery can be tested.

## UTM diagnostics: first use read-only preflight

As a regular KDE user:

```sh
cd ~/workspace/shell
git switch main && git pull --ff-only
cargo build --manifest-path rust/Cargo.toml --workspace --locked
./rust/target/debug/pokdrm
```

Keep a copy of `drm_cards`, connectors/status/modes,
`kms_readable`, `native_kms_candidate`, `session_type` and driver.
This tells us whether a native display can be tested *at all* under
UTM VirtIO GPU. CI may report `drm_cards=0`, which is not a hardware failure.

## Separate TTY/seat test (only where appropriate)

Install `libseat-dev libdrm-dev libgbm-dev pkg-config` for building.
On a real Linux TTY with an **active authorized logind/seatd seat**,
after verifying no KDE session is using the target DRM device:

```sh
cargo build --manifest-path rust/native/Cargo.toml
./rust/native/target/debug/poknative --card /dev/dri/card0
```

Expected diagnostic milestones are `POKNATIVE seat=...`,
`kms_crtcs=...`, at least one connector with modes,
and `gbm_initialized=yes`.

Do not launch `poknative` with sudo or from KDE's graphical session.
The output does **not** claim native fullscreen Wayland gaming,
modeset success, page flips, compositor rendering, or libinput/gamepad
ownership. Those are separate runtime acceptance gates.

## Next implementation gates

1. Libseat activation/pause-resume event processing, hotplug and DRM master
   lifetime with reliable non-destructive teardown.
2. GBM/EGL output buffers, CRTC/connector selection, modeset and vblank loop.
3. Move the existing Smithay XDG/layer-shell rendering core to the native
   backend without duplicating game/session IPC.
4. Libinput routing, multiple outputs and suspend/resume.
