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
matching vblank events. The blue and green frames are now each held for
half the requested duration AFTER their respective vblank events; previously
green immediately replaced blue. No new GPU draw happens during either hold.
The diagnostic exits automatically, or on Ctrl+C. On seat loss/hotplug/error it stops instead
of attempting further submissions. Expected logs: kms_buffered_surface_ready,
kms_first_frame_queued, kms_frame_presented=0, kms_frame_presented=1,
kms_scanout_test=complete. Confirm the original TTY comes back. If not,
switch VT or restart the graphical session; record the failure rather than
claiming successful restore. Do not run as root or inside KDE.
This is not a native Wayland game session yet.

## P1-N05 experimental native Wayland session

`poknative --wayland` uses the same XDG/layer-shell compositor core as
`pokcomp`, but queues composed frames to the native GBM/KMS swapchain.
It remains a single-output experiment; SHM clients, keyboard and mouse
are the initial target. DMA-BUF client negotiation, touch/controller
isolation, and hardware suspend/recovery are not yet complete. Do not
claim native game acceptance before the TTY scanout test passes.

## P1-N05 frame pacing and idle GPU policy

The shared Smithay compositor now records surface-tree damage and both
the KDE nested harness and the Linux-native backend avoid submitting
unchanged frames. On the KMS path `FramePump` permits **one in-flight**
scanout at a time; a corresponding `frame_submitted()`/vblank event must
arrive before the next render. Idle clients do not request new GPU buffers
or trigger redraws. The native run logs
`native_frames_submitted=N presented=M` on orderly shutdown.

This is a correctness/efficiency policy, **not a measured battery-life
result**: idle CPU wakeups, refresh scheduling and real GPU clock behavior
must still be benchmarked on hardware. Software CI tests the frame
state-machine, not real-world power consumption or atomic scanout.


### UTM KMS scanout user acceptance (2026-11-21)

The user supplied terminal logs confirming `kms_frame_presented=0`,
`kms_frame_presented=1` and `kms_scanout_test=complete`, plus a green
scanout photograph on VirtIO GPU / card0 (1280×800, 120 Hz). This establishes
an **actual UTM/VM KMS framebuffer scanout**, not a physical handheld test
or native Wayland game rendering. The original immediate blue-to-green swap
made blue effectively invisible; the diagnostic now holds each color.
`egl_device_software=true` and absent `EGL_EXT_device_drm` do not invalidate
the DRM scanout, but do not establish hardware-accelerated GLES performance.

## P1-N07 one-command native game + menu lifecycle (experimental)

The `poksession --native` CLI starts the seat-authorized `poknative
--wayland` server, waits for its socket announcement, then starts the
real `pokshelld` daemon (game pinned to the native `pokshell-*` Wayland
display) and `pokshell-layer --menu` on that **same** server.
The TTY user's Ctrl+C terminates only those child processes owned by
the supervisor. Do not run from KDE or as root. The game and layer do
not launch until the compositor has announced its real Wayland socket.

After compiling the Rust workspace, native backend and menu client:

```sh
./rust/target/debug/poksession --native --card /dev/dri/card0 \
  --game /usr/games/supertux
```

Expected `POKSESSION_READY backend=native`, SuperTux and menu on the
native KMS scanout, and `pokctl OVERLAY OPEN` to toggle the menu.
A separate TTY or SSH connection can issue pokctl commands. This is an
**experimental runtime acceptance**: do not mark native game/Overlay
presentation or libinput focus as passed until tested with UTM screenshots
and logs. CI verifies the process/socket lifecycle without access to DRM.

## P1-N05 black-screen incident: fail-closed native game startup

On 2026-11-21 the user reported that `poksession --native` displayed a
black screen and input appeared unresponsive, despite passing the previous
two-color native KMS test. **Do not claim P1-N05 accepted.** We do not
yet have compositor or SuperTux stderr from the failed run, so the exact
root cause is not confirmed.

The native backend now waits for an **actually drawable XDG game buffer**
before its first KMS framebuffer commit (instead of submitting the empty
dark background). If no game buffer arrives in 30 seconds it exits with
`no renderable game surface after 30s`, leaving the TTY unmodified by
this native compositor's scanout path. A hardware **Ctrl+Alt+Backspace**
(evdev) emergency shortcut sets the native shutdown flag independent
of game keyboard focus. CI covers the key-state policy and backend
build; real hardware/UTM input capture requires separate acceptance.

Before any further native test, recover access and start with a fresh
ordinary-user TTY, with KDE stopped. Capture logs for diagnosis, then
run *one* bounded attempt:

```sh
cd ~/workspace/shell
./rust/target/debug/poksession --native --card /dev/dri/card0 \
  --game /usr/games/supertux > "$HOME/pokshell-native.log" 2>&1
```

Use Ctrl+Alt+Backspace to leave a black-screen session. If it cannot be
delivered to the VM, use SSH or another TTY to terminate `poksession`
and subsequently start the display manager. A compositor that has
already performed modeset is **not** guaranteed to restore the previous
VT scanout without manual verification; do not repeat a black-screen
run without collecting its log first. No root compositor, no KDE
takeover, and do not treat CI headless output as hardware acceptance.
