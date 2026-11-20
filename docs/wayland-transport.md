# Actual Wayland graphics milestone: independent layer surface

**Status: development probe, not a pokshell compositor or polished UI.**

`graphics/wayland-layer` is a Rust **Wayland client** using the standardized wl_shm transport and the *wlroots-specific* `zwlr_layer_shell_v1` extension. It creates a right-edge **Overlay layer surface** under an existing compositor (e.g. sway/labwc), renders two immutable ARGB8888 buffers (transparent vs diagnostic panel) and listens to the Rust `pokshelld WATCH` socket stream.

When the daemon reports `OVERLAY OPEN`, the panel appears **over a separate native game surface**. `OVERLAY CLOSE` makes the layer transparent again without stopping the game. The overlay requests **no keyboard focus** and sets an empty input region, so input reaches the game.

This is a genuine Wayland surface, *not* a screenshot, framebuffer demo or an HTML/browser page. Its visual appearance is intentionally diagnostic and disposable.

## What this does not do

- It **does not run its own compositor**, grab DRM/KMS, or guarantee a Vulkan/GLES frame path. Graphics composition is provided by an external Wayland compositor.
- It is **not yet interactive**. Input routing/focus/clickable overlay are later steps.
- The layer-shell extension is not universal. Stock Weston generally does not provide `zwlr_layer_shell_v1` without additional support; use Sway or labwc for this validation.
- Transparent SHM content is a first proof, not a low-overhead GPU texture or the future Rust-native UI. Hidden mode performs **no drawing**, but a transparent surface still exists.
- It does not currently respond to multi-output policy or scale changes beyond a basic height configure.
- It **does not make the system bootable** on RG40XX H, Duo Lite or RP6. Board-specific DRM/KMS support must be verified separately.

## Debian ARM64 inside UTM

If you are in a minimal Debian console without a Wayland compositor, for a temporary graphics test session:

```sh
sudo apt update
sudo apt install -y sway cargo rustc python3 pkg-config dbus-user-session
# From a local Linux tty/console with working DRM/KMS + seat access:
dbus-run-session sway
```

Some UTM VirtIO GPU or login/seat configurations will not start Sway. Do not assume this command is portable to every virtual GPU configuration; inspect Sway logs if it fails.

Inside a terminal emulator in that Wayland session (e.g. `foot`), build the two **separate** Rust packages:

```sh
cd shell/rust
cargo build --workspace --locked
cd ../graphics/wayland-layer
cargo build
```

Open terminal A:

```sh
cd shell/rust
./target/debug/pokshelld --game /usr/bin/supertux
```

A configurable SDL/GLES application may be used instead of SuperTux. To get SuperTux from Debian, run `sudo apt install supertux`. Some SDL builds may require `SDL_VIDEODRIVER=wayland`; ensure the game launches in the same Wayland session.

Open terminal B:

```sh
cd shell/graphics/wayland-layer
cargo run
```

Open terminal C:

```sh
cd shell/rust
./target/debug/pokctl GAME START
./target/debug/pokctl OVERLAY OPEN
./target/debug/pokctl OVERLAY CLOSE
./target/debug/pokctl GAME STOP
```

Use `cargo run -- --show` in the graphics directory for a temporary **forced visible** panel independent of the IPC daemon, to debug layer-shell availability. It does not prove event wiring.

## Validation matrix

- `cargo test` and `cargo build`: check Wayland transport, format and event handling compile for x86-64 and native AArch64.
- **Not yet verified**: actual visible overlay above a running GLES game on UTM; no screenshot/test log has been supplied from a real compositor.
- Future CI should start a headless wlroots compositor and assert layer mapping, protocol/errors, output pixels, and no game interruption.
- Next architectural gate: own Rust Smithay-backed compositor and independent game/overlay surface management (not this temporary Wayland client under Sway).

## Source and license

Own code: GPL-3.0-or-later. The Wayland crates and their dependencies retain their upstream license terms.
