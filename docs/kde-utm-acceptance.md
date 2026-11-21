# KDE Plasma Wayland / Debian ARM64 UTM acceptance

This is a **manual acceptance procedure**. The project owner reported two checks passed in UTM on 2026-11-21: SuperTux now opens inside the nested Smithay window, and `pokctl QUIT` exits the game/session daemon without closing the separate Smithay window. Other checks below remain open until explicitly observed.
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
git switch main
git pull --ff-only

cargo build --manifest-path rust/compositor/Cargo.toml
cargo build --manifest-path graphics/wayland-layer/Cargo.toml
(cd rust && cargo build --workspace --locked && cargo test --workspace --locked \
    && python3 tests/ipc_integration.py)
command -v /usr/games/supertux || ls -l /usr/games/supertux
```

This sprint commits directly to `main`; no PR checkout is required.
If `git switch main` complains about uncommitted changes, save those
changes first rather than resetting or discarding them.

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
./rust/target/debug/pokshelld \
    --game /usr/games/supertux \
    --game-wayland-display pokshell-1
```

Expected: `pokshelld ready socket=/run/user/<UID>/pokshell.sock`.
The required `--game-wayland-display` explicitly targets pokcomp's
own socket (replace `pokshell-1` in both arguments with A's actual
printed socket name). It overrides inherited
`WAYLAND_DISPLAY` and `SDL_VIDEODRIVER`, removes `DISPLAY` and
`WAYLAND_SOCKET`, and refuses GAME START when the nested socket is missing.
The host KDE/KWin socket must **not** be used as the game socket.
If the daemon reports `socket already exists`, first ensure that no
`pokshelld` process is listening on that socket. **Never unlink a live
server's socket.** With the new signal handling, normal Ctrl+C/SIGTERM
also removes the daemon's own socket.

**C — independent interactive overlay**:

```sh
cd ~/workspace/shell
export WAYLAND_DISPLAY=pokshell-1  # same value as B, not KDE's host socket
./graphics/wayland-layer/target/debug/pokshell-layer \
    --socket "$XDG_RUNTIME_DIR/pokshell.sock" \
    --interactive --keyboard-interactive
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
game navigation key, or use the mouse in the game.

**New keyboard focus acceptance (2026-11-21 implementation; UTM still
unverified):**

1. With `OVERLAY OPEN`, **right-click the visible panel**. This
   focuses the panel without closing it (only a left click invokes the
   pointer-close action).
2. Press **Escape**. Expect `pokshell-layer interactive close requested
   via=escape` in terminal C and the panel to disappear.
3. Confirm the game PID remains unchanged and **game keyboard input
   responds again** without restarting SuperTux.
4. Repeat with `--interactive` but without `--keyboard-interactive`:
   the panel should remain click-to-close, while the game retains
   keyboard focus even after clicking the panel.

Finally, Ctrl+C the compositor (A) and layer client (C) if still running.

## Troubleshooting a SuperTux window that opens outside Smithay

1. Verify B was started with `--game-wayland-display pokshell-...`.
   **Restart B** after changing flags; restarting only SuperTux cannot
   change the daemon's configured launcher.
2. In D, run `pokctl STATUS`, note the PID, and inspect the game's
   environment from the same user session:

```sh
GAME_PID=$(./rust/target/debug/pokctl STATUS |
    sed -n 's/.*pid=\\([0-9][0-9]*\\).*/\\1/p')
test -n "$GAME_PID" && tr '\\0' '\\n' <"/proc/$GAME_PID/environ" |
    grep -E '^(WAYLAND_DISPLAY|SDL_VIDEODRIVER|DISPLAY|WAYLAND_SOCKET)='
```

Expected: `WAYLAND_DISPLAY=pokshell-...`,
`SDL_VIDEODRIVER=wayland`, and **no** `DISPLAY` or
`WAYLAND_SOCKET` entries. (The reported supervisor PID may be a
wrapper rather than the actual renderer; inspect its descendants if needed.)

3. A's terminal should report `pokcomp: new xdg_toplevel` and
   `pokcomp: game render elements present` after the game starts.
   If these are absent, the game is not submitting a rendered XDG
   surface to pokcomp.
4. A black game window **inside Smithay** is a different issue from a
   separate KDE game window. It indicates a possible SDL OpenGL/EGL,
   dmabuf or compositor render-path limitation; capture game stderr and
   `pokcomp` logs, and do not mark the graphics acceptance as passed.

## Acceptance checklist

- [ ] A runs nested in **KDE/KWin Wayland**, no DRM/KMS permission error.
- [x] SuperTux appears **inside pokcomp's host window**, rather than as a separate KDE window (reported 2026-11-21).
- [x] `pokctl QUIT` exits the game/session daemon but leaves the independently running Smithay host window open (reported 2026-11-21).
- [ ] The daemon runs as the expected regular user with explicit `--game-wayland-display` (configuration details not independently inspected in UTM).
- [ ] C connects to **pokcomp**, not the host KWin display.
- [ ] The game continues responding while the overlay appears.
- [ ] Clicking the visible panel closes it; the game PID remains unchanged.
- [ ] Clicking the game redirects pointer and keyboard input to the game.
- [ ] `--keyboard-interactive` panel receives Escape and closes via IPC;
      after the layer hides, keyboard focus returns to SuperTux.
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

## XDG fullscreen/resize acceptance

When the SuperTux menu is visible inside the Smithay window:

1. Resize the **outer Smithay KDE window** and verify the game resizes
   rather than remaining confined to a small top-left rectangle.
2. If SuperTux offers a fullscreen toggle, use it and verify that the
   **inner game** fills pokcomp's rendering extent, without causing a
   new KDE game window or stealing KWin's DRM/KMS device.
3. Toggle back and repeat `OVERLAY OPEN/CLOSE`; the overlay should
   remain on the right side and game PID should remain unchanged.
4. Run `pokctl QUIT` and verify that Smithay still runs independently.

CI has passed a software SDL2 fullscreen XDG request test; the real
SuperTux/OpenGL fullscreen and resize behavior remains a separate
UTM manual acceptance gate.
