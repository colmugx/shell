# Experimental pokshell-owned compositor: nested Smithay probe

This milestone adds `rust/compositor` (`pokcomp`), a **Wayland server**
implementing basic `wl_compositor`, `wl_shm`, `wl_seat`, `wl_output`, `xdg_shell`, and `zwlr_layer_shell_v1`
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
The daemon's `--game-wayland-display` **pins the child** to this nested
socket, verifies that the server is listening, and disables X11 fallback.
Using `WAYLAND_DISPLAY` alone was insufficiently robust in KDE.
Do not change the host desktop's `WAYLAND_DISPLAY` globally.

```sh
cd shell
export WAYLAND_DISPLAY=pokshell-1
export SDL_VIDEODRIVER=wayland
rust/target/debug/pokshelld --game /usr/games/supertux --game-wayland-display pokshell-1
```

In terminal C, start the existing separate Wayland Layer Client against
the **same nested compositor socket**. It uses the daemon's WATCH stream.
`--interactive` gives a visible diagnostic panel a pointer input region:
clicking the panel issues `OVERLAY CLOSE` via IPC. Without that flag the
older diagnostic panel remains click-through:

```sh
cd shell
export WAYLAND_DISPLAY=pokshell-1
cargo build --manifest-path graphics/wayland-layer/Cargo.toml
graphics/wayland-layer/target/debug/pokshell-layer --interactive
```

In terminal D, control the game and overlay:

```sh
cd shell
rust/target/debug/pokctl GAME START
rust/target/debug/pokctl STATUS
rust/target/debug/pokctl OVERLAY OPEN
rust/target/debug/pokctl OVERLAY CLOSE
rust/target/debug/pokctl GAME STOP
rust/target/debug/pokctl QUIT
```


**UTM manual acceptance (user-reported, 2026-11-21):** after pinning the game to pokcomp's own Wayland socket, SuperTux appears within the Smithay host window rather than in a separate KDE game window. `pokctl QUIT` stops the game/session daemon while the independent Smithay window stays open. This confirms the intended process lifetime boundary; it does not establish DRM/KMS ownership, GLES acceleration, reliable fullscreen policy, or interactive-overlay input acceptance.

## CI evidence

GitHub Actions runs a real headless Sway host, starts `pokcomp` as an
independent nested Wayland **server**, uses `pokshelld` to launch
`weston-simple-shm` as a separate native XDG client, and connects the
existing `pokshell-layer` with a live WATCH subscription. The test checks
that XDG game and layer render elements are produced, that layer buffers
switch from transparent to visible and back, and that the game PID remains
unchanged across the overlay transition.

The test also starts the layer with `--interactive`, exercising the
Wayland seat/pointer protocol binding, but CI does **not** synthesize a mouse
click; click-to-close requires the manual KDE/UTM acceptance test.

This is **automated software-renderer integration**; it does not prove
pixels on a physical display, hardware GPU acceleration, UTM performance,
pointer/controller focus, or a DRM/KMS-native shell.

## Deliberate limits

- **Nested only:** no DRM/KMS/GBM/libseat/udev session and no device boot test.
- The compositor now advertises `wl_output` and a first
  `zwlr_layer_shell_v1` implementation. It configures and renders a
  **single diagnostic right-side overlay** above XDG game surfaces; generalized
  layer policy, margins, layer stacking, shell surfaces on multiple monitors,
  and keyboard/gamepad overlay focus remain unimplemented.
- XDG toplevels overlap at the same origin. No placement, fullscreen
  negotiation, popup behavior, correct pointer hit testing, output scale,
  gamepad, touch, window occlusion, or multi-display policy.
- Pointer movement/button events are forwarded to surfaces according to
  Wayland input regions; clicking establishes keyboard focus. The optional
  interactive diagnostic layer can close itself through IPC on a left click.
  A full controller-driven menu, touch policy, pointer grabs and gamepad input
  remain unimplemented.
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
3. Validate the **same compositor** drawing SuperTux and the diagnostic layer
   while the game PID and frame callbacks survive `OVERLAY OPEN/CLOSE`.
4. Implement independent overlay input focus (rather than no-focus diagnostic
   surface) and hit testing without killing or hiding the game.
5. Add native Linux DRM/KMS backend with seat management, then verify
   actual UTM graphics and device support separately.

Smithay minimal example attribution and MIT notice:
[`rust/compositor/THIRD_PARTY_NOTICES.md`](../rust/compositor/THIRD_PARTY_NOTICES.md).


## Nested XDG fullscreen and output resize

The nested compositor's game-first XDG policy initially configures
new game toplevels as **Maximized** at the current host window extent.
When an XDG client requests fullscreen, it receives `Fullscreen` plus
a configure size matching the nested output. `unset_fullscreen` restores
maximized, and `unset_maximized` returns a normal unconstrained XDG
size. Resizing the host window sends a new configure to any mapped
maximized/fullscreen toplevel as well as the diagnostic layer.

`rust/compositor/tests/headless_nested.py --sdl2-fullscreen` launches a
real SDL2 client that calls `SDL_SetWindowFullscreen`, verifies Smithay
handled the XDG fullscreen request, and repeats game/overlay lifecycle
checks. This is **nested output fullscreen**, not the outer KDE window
entering fullscreen or DRM/KMS mode switching.

The current game-first policy remains a one-output single-game prototype.
It does not implement client-requested output selection, multiple windows
placement, popup constraints or multiple monitor geometry.
