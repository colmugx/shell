# pokshell

Game-first native Linux console-session research. **Not a completed Linux distribution.**

**Three reference platforms:** RG35XX H / RG40XX H (or H700-class equivalent), Retroid Pocket Duo Lite (QCS6125), Retroid Pocket 6. [Hardware baselines](docs/hardware-baselines.md).

## Approach

- Start on **a full ARM64 Linux distribution** (Debian in UTM, proven device-specific Linux on real hardware).
- Run a **standalone game-console session** rather than extend GNOME/KDE or write a desktop theme.
- Implement core services in **Rust**, component logic preferably in **MoonBit**, optional tiny scripts in **Lua**, and introduce TypeScript **only if absolutely necessary**.
- Native ARM64 executables remain the primary games; **WAMR Core Wasm** is for system/overlay components, not game binaries. WIT is not required.
- Kernel, boot chain, GPU drivers and hardware bring-up remain external board-specific work. Defer custom rootfs images until the userspace stack works.

Read [Linux-first strategy and language choices](docs/boot-and-languages.md) and [architecture](docs/architecture.md).

## v0.1 Rust bootstrap (current direction)

`rust/` contains a no-desktop, dependency-free Rust workspace:

- `pokshell-core`: native game-process supervision, state machine and logical overlay status
- `pokshelld`: per-session Unix socket daemon; executes **only the configured game executable**, with bounded client connections and live event subscribers
- `pokctl`: command-line control client, including live `WATCH` event stream
- unit tests and Linux IPC/process integration tests

**The Rust control plane is independent of the compositor.** It changes overlay state. A separate, experimental Wayland layer client can display a diagnostic surface under an existing wlroots compositor; the production shell is still in development. No PocketJS host, MoonBit runtime or WAMR worker is currently linked.

```sh
cd rust
cargo test --workspace --locked
cargo build --workspace --locked
python3 tests/ipc_integration.py

# Two terminals:
cargo run --bin pokshelld -- --game /bin/sleep --game-arg 120
cargo run --bin pokctl -- GAME START
cargo run --bin pokctl -- STATUS
cargo run --bin pokctl -- OVERLAY OPEN
cargo run --bin pokctl -- GAME STOP
cargo run --bin pokctl -- QUIT
```

Watch state changes in a third terminal with `cargo run --bin pokctl -- WATCH`. The event stream includes game started/exited and overlay open/closed. It is an in-memory live feed, not a durable event log.\n\nGitHub Actions exercises Rust and C baselines on x86-64 and native ARM64 Linux runners.

## Experimental native Wayland overlay transport

The standalone Rust client at [`graphics/wayland-layer`](graphics/wayland-layer) creates an independent wl_shm Layer Surface using the wlroots `zwlr_layer_shell_v1` protocol. It watches `pokshelld WATCH` for overlay state and supports both the old
transparent diagnostic surface and a `--menu` mode with text, selection,
keyboard/mouse navigation and fixed Resume/Restart/Stop commands. An optional
`--joystick /dev/input/js0` prototype reads Linux joystick events as a normal
user without exclusive device capture. This is not a production controller
input stack or handheld firmware. Headless Wayland CI verifies protocol
coexistence, while the user's KDE/UTM acceptance has confirmed game window
ownership, fullscreen resizing and keyboard focus restoration. [Build and UTM test instructions](docs/wayland-transport.md).

## Own compositor: first nested Smithay prototype

`rust/compositor` now contains an **experimental Smithay-backed Wayland server**,
separate from the proven `pokshelld` control plane and standalone
`graphics/wayland-layer` client. The initial Winit/GLES backend draws XDG
toplevels and diagnostic Layer Surfaces inside a host window. A software
headless CI integration test exercises a native XDG client, daemon IPC
and overlay visibility changes without terminating the game. An experimental
`--interactive` layer mode enables click-to-close, and nested compositor
pointer routing now honors surface input regions. Game launch with
`pokshelld --game-wayland-display pokshell-N` explicitly targets the
nested compositor's socket and disables KDE/X11 fallback (see
[UTM acceptance](docs/kde-utm-acceptance.md)). **The user has accepted the nested SuperTux game and the graphical
POKSHELL menu in the same Smithay window**, including keyboard navigation
and Resume/Restart/Stop actions (2026-11-21 UTM screenshot). The menu is
pinned to the nested server using `--display pokshell-N`. This does not
prove native DRM/KMS, exclusive physical gamepad focus, or hardware GLES
acceleration. See the
[compositor build and manual test guide](docs/compositor.md).

## Native Linux graphics work

The project is also implementing a **non-nested Linux display backend**:

- `rust/apps/pokdrm`: read-only `/sys/class/drm` enumeration and the
  kernel DRM KMS resource ioctl. This runs safely as the ordinary KDE user
  and identifies actual connectors/modes and access limitations.
- `rust/native`: standalone Smithay **libseat → DRM → GBM** bring-up.
  Requires an active, authorized non-root TTY seat, refuses KDE sessions
  and does not yet change KMS modes or draw Wayland client buffers.

See [native DRM/KMS bring-up and acceptance](docs/native-linux.md). The
nested GUI remains useful for functional tests; it is **not** the final
hardware display architecture.

## Historical C framebuffer experiment

The root `src/`, `include/`, `CMakeLists.txt` and `tests/` are a disposable **Linux fbdev technical demonstration**, **not** the project's intended product UI or future architecture. It draws a software UI, runs a standalone test game and shares frames through a demo buffer.

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
POCKET_BIN_DIR="$PWD/build" python3 tests/test_runtime.py
```

[UTM instructions](docs/utm.md) and Buildroot recipes remain exploratory. **No handheld-ready firmware or verified bootable QEMU image is present.** Do not flash QEMU images onto any target handheld.

## Project boundaries

See [verification](docs/verification.md), [license notes](docs/licensing.md) and [engineering rules](AGENTS.md). New original source is **GPL-3.0-or-later**; upstream components retain their licenses.

Name, design and system architecture remain subject to change. No proprietary firmware or game assets are committed.
