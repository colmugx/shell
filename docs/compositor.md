# Experimental pokshell-owned compositor: nested Smithay probe

This milestone adds `rust/compositor` (`pokcomp`), a **Wayland server**
implementing basic `wl_compositor`, `wl_shm`, `wl_seat` and `xdg_shell`
handling through Smithay. Its Winit/GLES backend draws client surfaces into
a **host Wayland or X11 window**. This is not the final DRM/KMS native session.

## Build

Requires Rust 1.82+ and a working Linux Wayland/X11 graphical session
with EGL/OpenGL ES and development libraries (`pkg-config`, Wayland,
EGL, XKB). The compositor is a standalone Cargo workspace so the
existing dependency-free `rust/` workspace and lockfile remain unchanged.

```sh
cargo build --manifest-path rust/compositor/Cargo.toml
cargo test --manifest-path rust/compositor/Cargo.toml
rust/compositor/target/debug/pokcomp --help
```

## Manual UTM / Debian experiment

Build the Rust supervisor independently and install a native Wayland game
such as SuperTux. From a terminal **inside an existing working graphical
session**:

```sh
cd shell
cargo build --manifest-path rust/compositor/Cargo.toml
(cd rust && cargo build --workspace --locked)

# Terminal A. Keep this process running; it creates a visible host window.
rust/compositor/target/debug/pokcomp
# Example output: WAYLAND_DISPLAY=pokshell-1
```

In terminal B, replace `pokshell-1` with the actual socket printed above.
The daemon inherits this environment; its native game process inherits it
in turn. Do not change the host desktop's own `WAYLAND_DISPLAY` globally.

```sh
cd shell
export WAYLAND_DISPLAY=pokshell-1
export SDL_VIDEODRIVER=wayland
rust/target/debug/pokshelld --game /usr/games/supertux
```

In terminal C:

```sh
cd shell
rust/target/debug/pokctl GAME START
rust/target/debug/pokctl STATUS
rust/target/debug/pokctl GAME STOP
rust/target/debug/pokctl QUIT
```

**Expected only after manual verification**: a native Wayland game can
connect to pokcomp, receive XDG configure events, submit buffers, and
be drawn in the compositor's host window. A successful Rust build or
CI test is **not** evidence of visible game frames in UTM.

## Deliberate limits

- **Nested only:** no DRM/KMS/GBM/libseat/udev session and no device boot test.
- No `zwlr_layer_shell_v1` support yet. The existing
  `graphics/wayland-layer` client will **not** work against this socket.
  Continue using Sway/labwc for that independent layer-shell probe.
- XDG toplevels overlap at the same origin. No placement, fullscreen
  negotiation, popup behavior, correct pointer hit testing, output scale,
  gamepad, touch, window occlusion, or multi-display policy.
- Keyboard focus is provisional and selected on absolute pointer motion.
  This is not yet suitable for a game/interactive overlay coexistence test.
- No dmabuf advertisement or verified Vulkan/GLES client zero-copy path.
  The GLES backend is only the nested compositor's output renderer.
- The compositor **does not spawn** arbitrary commands or replace
  `pokshelld`. Games are started by the existing fixed-executable
  supervisor.
- The listener uses a per-user `XDG_RUNTIME_DIR` Wayland socket with
  collision-safe name selection. Run only as a nonprivileged user.

## Next engineering gates

1. CI compilation on x86-64 and ARM64; verify socket/XDG configure on a
   real nested graphical session.
2. Add `wl_output`, xdg surface placement and proper pointer/keyboard
   focus routing before claiming practical game controls.
3. Add a separate overlay surface with independent interactivity; retain
   game PID and frame callbacks across `OVERLAY OPEN/CLOSE`.
4. Add native Linux DRM/KMS backend with seat management, then verify
   actual UTM graphics and device support separately.

Smithay minimal example attribution and MIT notice:
[`rust/compositor/THIRD_PARTY_NOTICES.md`](../rust/compositor/THIRD_PARTY_NOTICES.md).
