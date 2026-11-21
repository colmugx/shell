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
  a compatible connector/encoder/CRTC route, GBM + EGL + GLES renderer initialization,
  udev DRM hotplug monitoring, libinput event collection and libseat
  suspend/resume handling. **Default probe does not modeset; opt-in bounded scanout test is implemented, native Wayland presentation remains pending**. It refuses to run inside KDE by default. It is intended
  for a separate native Linux TTY, not a nested KDE window.

The default probe leaves display modes alone. Only the explicitly requested bounded scanout test changes output state; restore must be verified in TTY.

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

Install these development dependencies before building (ordinary user
for Cargo; sudo only for installing packages):

```sh
sudo apt install -y pkg-config libdrm-dev libgbm-dev libseat-dev \
  libudev-dev libinput-dev libxkbcommon-dev libegl1-mesa-dev libgles2-mesa-dev
```
On a real Linux TTY with an **active authorized logind/seatd seat**,
after verifying no KDE session is using the target DRM device:

```sh
cargo build --manifest-path rust/native/Cargo.toml
./rust/native/target/debug/poknative --card /dev/dri/card0
```

Expected diagnostic milestones are `POKNATIVE seat=...`,
`kms_crtcs=...`, at least one connector with modes,
and `kms_route` (compatible connector/encoder/CRTC), `gbm_initialized=yes`,
`egl_display_initialized=yes`, and `gles_renderer_initialized=yes`.

Do not launch `poknative` with sudo or from KDE's graphical session.
The output does **not** claim native fullscreen Wayland gaming,
modeset success, page flips, compositor rendering, or libinput/gamepad
ownership. Those are separate runtime acceptance gates.

## Next implementation gates

1. Wire seat activation/hotplug signals to GPU output lifetime,
   including safe teardown on pause, resume and virtual-terminal changes.
2. Create GBM/EGL GLES render targets for selected CRTCs, then implement
   atomic modesetting and vblank-paced page flips.
3. Move the existing Smithay XDG/layer-shell surface composition to the
   native backend without duplicating pokshelld game/session IPC.
4. Complete libinput routing, multi-output geometry and suspend/resume
   for an actual handheld or dedicated Linux console session.

### Seat events and device hotplug

Run from an authorized Linux TTY (never from KDE):

```sh
./rust/native/target/debug/poknative --card /dev/dri/card0 --monitor-seconds 30
```

This processes libseat activation/pause, udev DRM hotplug, and libinput
device notifications. Use Ctrl+C to exit. It **does not** capture or
redirect game input or do modesetting. The monitor is a bounded diagnostic
used to validate the native Linux device/event stack.


## P1-N04: opt-in first two native scanout frames

The user's UTM preflight image (2026-11-21) shows virtio-pci,
/dev/dri/card0, Virtual-1 connected, 1 CRTC, 1 encoder and
native_kms_candidate=yes, with 1280x800 among the enumerated modes.
This is read-only preflight evidence, not a successful modeset.

Build the native backend in KDE first; then log out of KDE and use an
authorized non-root text TTY (not a Konsole window). Run:

```sh
./rust/native/target/debug/poknative --card /dev/dri/card0 --scanout-seconds 5
```

The opt-in test requires an atomic KMS device with exactly one connector
and CRTC. Smithay chooses a compatible primary plane/GBM format, renders
two distinct colors into dma-buf-backed buffers, queues them and consumes
matching vblank events. It holds the last frame without redraw; exits
automatically, or on Ctrl+C. On seat loss/hotplug/error it stops instead
of attempting further submissions. Expected logs: kms_buffered_surface_ready,
kms_first_frame_queued, kms_frame_presented=0, kms_frame_presented=1,
kms_scanout_test=complete. Confirm the original TTY comes back. If not,
switch VT or restart the graphical session; record the failure rather than
claiming successful restore. Do not run as root or inside KDE.
This is not a native Wayland game session yet.
