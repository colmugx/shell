# KDE Plasma Wayland / Debian ARM64 UTM acceptance

This is a **manual acceptance procedure**, not evidence that UTM was tested.
Run as the *logged-in non-root Plasma user*. Do **not** launch a DRM/KMS
compositor, run `sudo sway`, or replace the current KWin session.

## Preconditions and build

Open a fresh regular-user Konsole:

```sh
id -u
printf 'session=%s display=%s runtime=%s\n' "$XDG_SESSION_TYPE" "$WAYLAND_DISPLAY" "$XDG_RUNTIME_DIR"
test "$XDG_SESSION_TYPE" = wayland
test -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" && echo "host Wayland OK"
```

An interactive login normally has a nonzero UID and a runtime directory
such as `/run/user/1000`. Do not continue if `id -u` prints `0`.

Install build prerequisites using `sudo` only for package installation:

```sh
sudo apt update
sudo apt install -y git build-essential pkg-config cargo rustc \
    libwayland-dev libegl1-mesa-dev libgles2-mesa-dev \
    libxkbcommon-dev libgl1-mesa-dri supertux
```

**Compiler:** `rustc --version` must be at least 1.82. If Debian ships
an older compiler, install a supported stable Rust toolchain for the normal
user. Do not build the project with `sudo`.

```sh
mkdir -p "$HOME/workspace"
cd "$HOME/workspace"
git clone https://github.com/colmugx/shell.git 2>/dev/null || true
cd shell
git fetch origin
git switch feat/session-input-and-clean-shutdown \
    || git switch -c feat/session-input-and-clean-shutdown --track origin/feat/session-input-and-clean-shutdown

cargo build --manifest-path rust/compositor/Cargo.toml
cargo build --manifest-path graphics/wayland-layer/Cargo.toml
(cd rust && cargo build --workspace --locked && cargo test --workspace --locked \
    && python3 tests/ipc_integration.py)
command -v /usr/games/supertux || ls -l /usr/games/supertux
```

The above branch is the **development/PR branch**. After merge it is
appropriate to switch back to `main` instead of tracking the PR branch.

## Four Konsole terminals

All four are launched from the logged-in regular KDE Wayland session.

**A — compositor** (keep running):

```sh
cd ~/workspace/shell
./rust/compositor/target/debug/pokcomp
```

Expected: a dark **separate KDE window** and output such as
`WAYLAND_DISPLAY=pokshell-1`. The number can differ. Write down the
**actual** socket name. It is not the KDE host display name.

**B — session daemon**:

```sh
cd ~/workspace/shell
export WAYLAND_DISPLAY=pokshell-1  # replace with A's actual output
export SDL_VIDEODRIVER=wayland
./rust/target/debug/pokshelld --game /usr/games/supertux
```

Expected: `pokshelld ready socket=/run/user/<UID>/pokshell.sock`.
If the daemon reports `socket already exists`, first ensure that no
`pokshelld` process is listening on that socket. **Never unlink a live
server's socket.** With the new signal handling, normal Ctrl+C/SIGTERM
also removes the daemon's own socket.

**C — independent interactive overlay**:

```sh
cd ~/workspace/shell
export WAYLAND_DISPLAY=pokshell-1  # same value as B, not KDE's host socket
./graphics/wayland-layer/target/debug/pokshell-layer \
    --socket "$XDG_RUNTIME_DIR/pokshell.sock" --interactive
```

Expected: layer Configure log. While overlay is closed it is transparent
and should not intercept input.

**D — control**:

```sh
cd ~/workspace/shell
./rust/target/debug/pokctl GAME START
./rust/target/debug/pokctl STATUS
./rust/target/debug/pokctl OVERLAY OPEN
./rust/target/debug/pokctl STATUS
```

Expected: game launches **inside A's dark window**, and a 320-pixel
right-edge diagnostic panel appears in the **same window** without
stopping the game. Click the visible panel with the mouse. The panel
should close (via `OVERLAY CLOSE`), and the game process should
remain alive with **the same** PID as before the click.

You can also test command-driven visibility:

```sh
./rust/target/debug/pokctl OVERLAY OPEN
./rust/target/debug/pokctl OVERLAY CLOSE
./rust/target/debug/pokctl STATUS
./rust/target/debug/pokctl GAME STOP
./rust/target/debug/pokctl QUIT
```

Check that a click in the game area focuses the game. Press a simple
game navigation key, or use the mouse in the game. The diagnostic
layer with `--interactive` only supports **click-to-close**, not a
complete menu or gamepad interface.

Finally, Ctrl+C the compositor (A) and layer client (C) if still running.

## Acceptance checklist

- [ ] A runs nested in **KDE/KWin Wayland**, no DRM/KMS permission error.
- [ ] B's daemon is the *same user* and can start/stop SuperTux.
- [ ] C connects to **pokcomp**, not the host KWin display.
- [ ] The game continues responding while the overlay appears.
- [ ] Clicking the visible panel closes it; the game PID remains unchanged.
- [ ] Clicking the game redirects pointer and keyboard input to the game.
- [ ] Ctrl+C on B removes `$XDG_RUNTIME_DIR/pokshell.sock`.
- [ ] Re-running B does not require manual deletion of the old socket.

If graphical startup fails, capture these non-sensitive diagnostics:

```sh
echo "$XDG_SESSION_TYPE / $WAYLAND_DISPLAY / $XDG_RUNTIME_DIR"
id -u
rustc --version
ls -l "$XDG_RUNTIME_DIR"/pokshell* 2>/dev/null
pgrep -af 'pokcomp|pokshelld|pokshell-layer|supertux'
```

Also retain the terminal A/B/C log output and a screenshot of the game
and overlay in the same nested window. Avoid sharing private environment
variables or unrestricted service logs without reviewing them.

**Not accepted by this procedure:** native DRM/KMS, hardware GLES/Vulkan,
Wayland dmabuf zero-copy, SuperTux fullscreen negotiation, gamepad/touch
routing, dual-display policy or handheld firmware.
