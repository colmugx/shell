# Verification status

Evidence is tracked per implementation, per machine and per hardware reference.

| Capability | Evidence | Status |
| --- | --- | --- |
| Legacy C framebuffer alpha on x86-64/ARM64 | GitHub Actions Linux builds/tests | CI verified |
| Rust process supervision, termination grace, event subscription, fragmented IPC | `rust/` unit/integration tests; Actions run 44695342191 | CI passed on x86-64 / ARM64 |
| Rust user session in Debian ARM64 / UTM | User-reported KDE Wayland VM session running pokcomp and SuperTux (2026-11-21); no boot log or native DRM/KMS evidence | Nested session observed by user; boot/native DRM/KMS unverified |
| UTM read-only KMS preflight | User screenshot 2026-11-21: virtio-pci card0, Virtual-1 connected, 1 CRTC/connector/encoder, kms_readable=yes and native_kms_candidate=yes | Preflight accepted; separately, native KMS scanout/page-flip now user-observed (see next row) |
| Bootable Buildroot QEMU image | No QEMU boot log | Unverified |
| UTM native DRM/KMS scanout (P1-N04 diagnostic) | User screenshot 2026-11-21: green screen at 1280×800 (120 Hz); terminal confirms libseat seat0, GBM/EGL/GLES initialized, KMS page-flip acknowledgments frame 0 and 1, and kms_scanout_test=complete on /dev/dri/card0 | Native VM scanout accepted; blue duration was too short to see; new two-color hold needs retest. Not proof of native Wayland game or hardware GPU acceleration |
| Own DRM/KMS and Wayland compositor | Smithay Winit nested compositor passed automated XDG/Layer Shell tests and user-reported UTM display acceptance. Native Linux pokdrm kernel DRM ioctl probe and separate Smithay libseat/DRM/GBM device bring-up source added; opt-in GBM/GLES atomic scanout diagnostic implemented; native XDG/SHM Wayland presentation observed on UTM; native diagnostic scanout accepted on UTM VirtIO VM | Nested graphics accepted; native Linux seat/DRM/GBM/EGL/udev/libinput compiled on ARM64/x86-64 CI; VM/UTM native XDG/SHM scanout observed; native SuperTux/OpenGL and physical handheld unverified |
| Wayland layer-shell diagnostic surface | Rust wl_shm + zwlr_layer_shell_v1 client; Actions 44704676668, x86-64 headless Sway protocol test | Compiled on x86-64 / ARM64; headless protocol handshake passed |
| UTM Wayland graphical session | User-reported acceptance on 2026-11-21: SuperTux is rendered within the nested Smithay host window; QUIT ends the game/session daemon while the separate Smithay host window remains open | Manual UTM acceptance passed for window ownership and process separation only |
| Actual game overlay during arbitrary fullscreen SDL/GLES game | Headless CI verifies XDG/Layer SHM coexistence and unchanged PID. Earlier UTM capture (2026-11-20) showed SuperTux escaping to KDE; following explicit game Wayland socket pinning, user reports SuperTux now inside the Smithay window (2026-11-21). Fullscreen negotiation previously unverified; nested XDG maximize/fullscreen configure and output-resize behavior now implemented with an SDL2 CI regression. the user subsequently reported working fullscreen/resize, Escape-close and game keyboard focus recovery in UTM. GLES acceleration remains unmeasured | User-reported UTM acceptance (2026-11-21) now includes SuperTux and graphical system menu together inside Smithay after explicit --display pinning; visual coexistence/navigation and menu actions accepted. Physical GPU acceleration and DRM/KMS native output unverified |
| MoonBit/WAMR component execution | Not linked | Not implemented |
| Production console UI, controller focus, dual screen | Nested pointer hit testing and click-to-focus implemented; optional panel click-to-close and OnDemand keyboard focus/Escape dismissal (CI protocol/lifecycle and user-reported UTM validation on 2026-11-21). Optional nonexclusive Linux joystick test path; no robust gamepad/touch, production UI or dual-screen policy | New Wayland SHM system menu with fixed Resume/Restart/Stop actions, keyboard/mouse navigation and opt-in /dev/input/js* joystick mapping. CI tests menu focus, IPC actions and joystick event decoding; menu UTM mouse/keyboard acceptance passed (2026-11-21); hardware joystick UTM acceptance pending. No exclusive gamepad isolation or production UI |
| H700, Duo Lite, RP6 physical-device boot / suspend / hardware acceleration | No test reports | Unverified |

**Evidence boundary:** CI verifies native Linux user-space behavior. The 2026-11-21 UTM result is a **user-reported manual acceptance** of nested SuperTux window ownership and independent compositor lifetime, not an independently captured boot log, GPU acceleration measurement, or full-screen/gamepad validation.

## Security/maintenance note

Current Rust daemon uses a user-owned local Unix socket (0600), fixed configured game executable, no shell interpolation, and no TCP API. It is a prototype; it does not yet implement sandboxing, privilege separation, durable signal-independent service restart supervision, durable/replayable events, or a robust cgroup-based descendant process lifecycle. `GAME STOP` uses process-group SIGTERM and a short SIGKILL escalation window. Termination of descendants after the main game process exits independently remains to be addressed. Do not expose this as a privileged system-wide service.

The historical C framebuffer program is not suitable as a production display manager.

P1-N05: shared XDG/layer-shell core reused by nested and native binaries;
native XDG/SHM scanout observed in UTM; SuperTux/OpenGL and physical input remain unverified.

P1-N05 energy policy: common XDG/layer compositor now skips unmodified
nested redraw; native `FramePump` retains at most one in-flight scanout
and waits for an acknowledged vblank before reusing buffers.
Proof: deterministic unit tests and x86-64/ARM64 CI pending; idle
power, GPU clocks and frame latency not measured.

### P1-N05 XDG/SHM canary (x86-64/ARM64 CI passed; UTM manual accepted)

pokxdg-smoke is a Wayland XDG-Shell + wl_shm test game. It does not link
to SDL, EGL, OpenGL or GLX. It performs bufferless XDG configure/ack and
attaches one bounded SHM checkerboard buffer; a frame callback is evidence
that the compositor notified the client after presentation.

Build using:
    cargo build --manifest-path graphics/wayland-layer/Cargo.toml
    cargo build --manifest-path rust/Cargo.toml --workspace --locked
    cargo build --manifest-path rust/native/Cargo.toml

On an authorized non-root TTY after stopping KDE, run:
    ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game "$PWD/graphics/wayland-layer/target/debug/pokxdg-smoke" \
      > "$HOME/pokshell-native-canary.log" 2>&1

Exit with Ctrl+Alt+Backspace. The expected trace includes connected,
new xdg_toplevel, xdg_configured, xdg_shm_buffer_committed,
native_kms_first_frame_queued=yes, and frame_callback_done.
Success isolates a SuperTux/SDL/EGL-specific problem; failure isolates
the native Wayland XDG/SHM/KMS path. 2026-11-21 UTM screenshots and console log confirmed 1280x800 XDG SHM checkerboard, layer/game GLES elements, KMS first frame, vblank=1, frame callback and submitted=1 presented=1. Later BrokenPipe followed Ctrl+Alt+Backspace shutdown. This does not prove SuperTux/OpenGL or DMA-BUF import. CI: https://github.com/colmugx/shell/actions/runs/44917016644 .

### P1-N05 SDL2 SHM-vs-OpenGL diagnostic (source change; CI and UTM pending)

The existing SDL2 Wayland smoke client has an opt-in "--opengl" mode.
The default mode stays SDL_GetWindowSurface -> wl_shm. Both modes print
staged diagnostics. The GL mode requests a desktop OpenGL context through
SDL2, clears a blue frame and calls SDL_GL_SwapWindow. This probes
native Wayland EGL and buffer import without modifying the game.

After building the native and Rust workspace binaries, run these
separately on an authorized non-root TTY without KDE:

    ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game /usr/bin/python3 \
      --game-arg "$PWD/rust/compositor/tests/sdl2_client.py" \
      > "$HOME/pokshell-sdl-shm.log" 2>&1

    ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game /usr/bin/python3 \
      --game-arg "$PWD/rust/compositor/tests/sdl2_client.py" --game-arg --opengl \
      > "$HOME/pokshell-sdl-gl.log" 2>&1

Then test the actual game with its documented renderer selector:

    ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game /usr/games/supertux2 \
      --game-arg --renderer --game-arg sdl \
      > "$HOME/pokshell-supertux-sdl.log" 2>&1

Use Ctrl+Alt+Backspace to exit each run if necessary. Examine markers
pokshell-sdl: xdg_window_created, shm_surface_submitted,
gl_context_ready, gl_swap_submitted; compositor new xdg_toplevel,
native_kms_first_frame_queued, frame_callback_done, and any SDL/EGL
errors. SuperTux's SDL renderer is not assumed to use wl_shm.

If SDL2/SHM passes but GL fails: inspect EGL/DMABUF buffer exchange.
If SDL2/GL passes but SuperTux fails: inspect game GL requirements.
If SDL2/SHM fails but raw XDG/SHM passed: inspect SDL's Wayland flow.

### P1-N05 protocol-stage stall (2026-11-21 UTM screenshots)

User provided UTM logs for SDL2/SHM and SuperTux (SDL renderer attempt).
Both accepted a fourth Wayland connection and remained alive, yet showed
xdg_toplevels=0, pending_dmabufs=0, native_frames_submitted=0 and no XDG role.
The SDL2 test did not print video_initialized, a marker after SDL_Init(VIDEO).
The working pokxdg-smoke proof already isolates an XDG/SHM server path that
does reach KMS scanout; current hypothesis is an SDL2 video initialization
stall, **not** a proven EGL/DMA-BUF import failure. Ctrl+Alt+Backspace yielded
the expected downstream BrokenPipe and killed owned game process.

Additional diagnostics:
- The native server startup status reports dispatched_requests in addition
  to accepted_clients, distinguishing a connected but silent client from a
  client actively exchanging registry/sync requests.
- The SDL2 ABI test prints before_video_init and accepts --trace-wayland,
  which sets WAYLAND_DEBUG=client inside the test process before SDL_Init.
- It also accepts --no-libdecor, which sets SDL_VIDEO_WAYLAND_ALLOW_LIBDECOR=0
  only inside that test client to isolate possible libdecor initialization.
  Neither option is applied automatically to games or production processes.

Repeat the **SDL2 SHM** test first, using both diagnostic arguments:

    ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game /usr/bin/python3 \
      --game-arg "$PWD/rust/compositor/tests/sdl2_client.py" \
      --game-arg --trace-wayland --game-arg --no-libdecor \
      > "$HOME/pokshell-sdl-init-trace.log" 2>&1

To check whether disabling libdecor affects SuperTux independently, run:

    SDL_VIDEO_WAYLAND_ALLOW_LIBDECOR=0 \
      ./rust/target/debug/poksession --native --card /dev/dri/card0 \
      --game /usr/games/supertux2 --game-arg --renderer --game-arg sdl \
      > "$HOME/pokshell-supertux-no-libdecor.log" 2>&1

A Wayland trace captures protocol requests/replies but does not identify
a compositor GPU issue without evidence of a window/buffer. CI builds and
headless tests do not establish the native SDL/GL outcome.

### P1-N05 autonomous native-event-loop protocol regression

A new protocol-only harness (pokwayland-protocol) uses the shared Smithay
Wayland Server and the same calloop read-ready dispatch used by the native
backend. It creates no EGL renderer, DRM device, libseat session or frame.
The existing SDL2 Wayland fixture has an opt-in --window-only mode to test
SDL_Init(VIDEO), SDL_CreateWindow, and XDG toplevel without a renderer.
The Linux x86-64/ARM64 compositor CI jobs now build this harness and run a
bounded Python integration test; no user TTY interaction is required.

Native mode's Wayland FD callbacks now directly dispatch client requests
on readiness instead of solely returning PostAction::Continue. This avoids
depending on an unrelated DRM/seat/input wakeup to progress a blocking
SDL wl_display_roundtrip. Native user-visible SDL/SuperTux success remains
unverified until physical/UTM evidence, distinct from the new CI gate.

### P1-N05 single-command temporary TTY acceptance

The checked-in executable `tools/tty` replaces multi-command manual
native tests. Prepare with `./tools/tty prep` in a graphical terminal
before stopping KDE. Run `./tools/tty` as a normal user from an
authorized text TTY. The script refuses graphical sessions and root,
checks prerequisites before DRM takeover, runs bounded SDL SHM/default,
SDL SHM/no-libdecor, SDL OpenGL/no-libdecor, and SuperTux SDL scenarios,
then stores per-case logs and a summary under a private HOME directory.
It does not run sudo, disable KDE, delete sockets or touch other sessions.
It remains temporary until manual physical/UTM acceptance, then is
removed in the following development iteration.

CI invokes its `--self-test` only (no DRM); x86-64 and ARM64 SDL2 protocol
regressions now check both the normal SDL video initialization path and
the no-libdecor variant. Neither CI path proves native KMS SuperTux.

### P1-N05 isolated DMA-BUF advertising comparison (diagnostic only)

The 2026-11-21 TTY summary reported four 22-second timeouts with
PRE_XDG_STALL_OR_EXIT. Status 124 is GNU timeout, not a compositor crash or
a measured EGL failure. Per-case logs remain required to distinguish
SDL_Init, Wayland sync, and protocol requests; the screenshot alone does
not establish which stage stalled.

Added opt-in `--no-dmabuf` to `poknative --wayland` and to
`poksession --native` (forwarded only to the owned native compositor).
It omits linux-dmabuf global registration for this session only, without
changing GLES/KMS or the default OpenGL path. CLI tests reject use outside
native Wayland and duplicate flags. The mock native session integration
now checks that the flag reaches its child compositor, without DRM access.
Temporary `tools/tty` adds an SDL/SHM comparison with and without
the DMA-BUF global and prints accepted Wayland clients, dispatched request
count and XDG toplevel count for each attempt. These outputs must not be
treated as hardware acceptance until UTM evidence arrives. Script stays
temporary until successful TTY validation, and will be removed in the
next iteration once the results support it.

### P1-N05 TTY script source/binary consistency gate

The user-facing 2026-11-22 UTM summary showed four timeouts (124) and
one immediate exit (2) for the new `--no-dmabuf` comparison, all without
a visible XDG window. The non-timeout exit cannot be attributed to
EGL/DMABUF without its log; notably, `git pull` does not rebuild Rust
binaries, which can leave an updated script invoking a stale binary
that does not recognize `--no-dmabuf`.

`tools/tty prep` now writes the built Git revision to a private,
untracked target-directory stamp only after all three builds succeed.
`tools/tty` refuses to run before a matching stamp is present, so an
outdated binary cannot masquerade as a Wayland capability failure.
The report distinguishes SDL_Init blocking, non-timeout option/launch
errors and actual XDG/first-frame failures. CLI failures display one
short actionable error; raw logs remain available.

Protocol-only CI now verifies that a successful SDL2 XDG handshake
increases the server's dispatched request counter (not merely that
clients connect). This is not evidence of native KMS scanout.

### P1-N05 backend dispatch counter correction (CI regression)

CI demonstrated that an SDL2 client created a valid XDG toplevel and
received its configure while Display::dispatch_clients() reported zero.
The system/libwayland backend's return count is not a reliable count of
handled protocol requests. Previous TTY summaries with "requests=0" were
therefore misleading; they do NOT prove zero client requests.

The shared compositor now records a monotonic count of actual XDG,
layer-shell and wl_surface commit callbacks; native startup reports
`protocol_callbacks` separately from the renamed
`backend_dispatch_count`. The headless SDL2 test asserts that this
protocol callback count increases on actual XDG creation. This still
does not measure every registry/sync request, but does provide genuine
server-side protocol progress independent of backend return semantics.

### P1-N05 native game-first ordering and TTY test short-circuit

UTM screenshot reported SDL_VIDEO_INIT_BLOCKED with callback count=3
and XDG toplevels=0 for all SDL variants, including SHM-only and
libdecor-disabled. Those callbacks may be entirely from the system
layer; they do not establish that SDL received its wl_display.sync
response. Disabling DMA-BUF and libdecor has not changed the failure.

Native session now sends GAME START to pokshelld before launching the
system menu client, preventing initial layer-shell configure from being
an explicit dependency of the game's first Wayland exchange. Nested
session ordering remains unchanged. A mocked native-session integration
test proves the actual game process exists before mock-layer starts;
no DRM master/seat operations occur in CI.

The temporary one-command TTY script now stops after the first failed
SDL SHM baseline, preserving its log and summary rather than running
four more known-redundant variants. A later release should replace
the diagnostic smoke test with a self-reporting first-frame acceptance
signal from poknative; the script remains temporary, pending user
acceptance, and the outcome is not yet a native SuperTux pass.

### P1-ROCKNIX external Sway compatibility backend (CI-probed; handheld unverified)

ROCKNIX/distribution (upstream `next` branch) uses a system-level Sway
service with WLR_BACKENDS=drm,libinput and provides seatd and Wayland
SDL2 support. See the upstream project at:
https://github.com/ROCKNIX/distribution/tree/next

Do NOT vendor ROCKNIX image/build scripts or brand artwork. ROCKNIX
is a distribution, not a Rust crate; upstream components have distinct
licenses. This original adapter reuses an **already-running** Sway
Wayland compositor as a temporary compatibility/display backend. It is
not PocketJS or the target dedicated pokshell KMS compositor.

The explicit external mode is opt-in:
- `tools/rocknix-session --display wayland-1 --game /usr/games/supertux2`
  while running as the unprivileged user in an existing ROCKNIX/Sway
  graphical session (use the actual socket basename as --display).
- The launcher never starts/stops Sway or calls libseat/DRM; it owns only
  pokshelld, its game and pokshell-layer. Shutdown terminates owned
  processes and leaves the compositor/socket intact.
- `pokshelld --allow-external-wayland` is accepted only alongside an
  explicit game Wayland display. It pins SDL/GDK/Qt to that socket and
  removes inherited DISPLAY and WAYLAND_SOCKET.
- `pokshell-layer --allow-external-wayland --display NAME` permits the
  opt-in non-pokshell display, without making KDE fallback implicit.
  The original --menu restriction remains the default.
- Reject missing/non-socket/symlink/path-traversal endpoints and existing
  pokshell IPC sockets. Never remove another session's socket.

CI launches a separate **headless Sway** and tests actual SDL2/SHM window
plus pokshell layer-shell configure, then signals only the launcher
and checks Sway/socket still alive. This measures protocol compatibility
and lifecycle, not visible pixels, real input, hardware acceleration or
ROCKNIX physical-device support. Native Smithay remains the main P1
long-term graphics owner. The ongoing `tools/tty` script is preserved
until the native renderer is genuinely accepted.

### P1-ROCKNIX Rust session consolidation (2026-11-22)

`poksession --external-wayland SOCKET --game /absolute/game` is the canonical
Rust supervisor for pre-existing Sway/ROCKNIX. It validates the explicit
socket and user, refuses DRM/compositor override combinations, owns only
pokshelld, its game and the layer menu, and leaves Sway untouched.

`tools/rocknix-session --display SOCKET --game /absolute/game` remains an
exec-only compatibility wrapper, eliminating duplicate signal handling and
lifecycle code. The x86-64 headless Sway integration test invokes Rust
poksession directly and asserts SDL2 SHM submission, layer configure and
that external Sway survives after pokshell shutdown. Physical ROCKNIX
graphics and real gamepad behavior remain unverified.
