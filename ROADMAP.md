# pokshell — Product Roadmap / 产品与开发路线

> **Source of truth.** All agents must treat this document and [AGENTS.md](AGENTS.md)
> as the project contract. The goal is a **next-generation, low-overhead gaming
> OS for Linux handhelds**, not a KDE theme, generic desktop, or emulator launcher.
> Updated: 2026-11-21. Milestones describe engineering *gates*, not delivery dates.

## North star / 最终目标

Boot directly into a **PocketJS system shell**, own Linux display/seat/input/
power policy in a small **Rust system core**, and deliver native games,
compatibility-layer games and emulator **cores** through one low-overhead
graphics/audio/input/lifecycle pipeline. Keep the kernel, device drivers,
firmware and the smallest *required* Linux userspace; eliminate redundant
desktop, launcher and emulator frontend processes where technically feasible.

A smaller binary, fewer processes and zero-copy are **hypotheses**, not success
metrics. Validate actual frame latency, RAM, wakeups, power, sleep/resume and
battery life against a reproducible baseline. Don't sacrifice correctness,
driver support or game compatibility for aesthetic minimalism.

### Target architecture (logical responsibilities, not required processes)

```text
Boot / device-specific kernel + firmware
  -> minimal Linux userspace + seatd/logind alternative, DRM, Mesa, ALSA/PipeWire*
     -> pokshell system core (Rust): session / launch / sandbox / power
        -> Smithay-native DRM/KMS compositor: GBM/EGL/Vulkan path, input, outputs
           -> Game runtime: Linux SDL/GLES/Vulkan, compatibility via Proton/Wine
           -> Emulator Core Host: Libretro-style ABI + per-core adapters
           -> PocketJS shell: Home / Library / Settings / Overlay / Plugins
              ^ unified video buffers, input focus, audio, save states & IPC
```

*Use only the audio/session components justified by hardware and application
requirements. No KDE, GNOME, browser, Electron or independent emulator frontend
in a production boot. Native game process isolation remains available; merging
every component into one process is **not** a requirement.

PocketJS is [pocket-nexus/pocketjs](https://github.com/pocket-nexus/pocketjs).
Its application toolchain uses JavaScript/TypeScript and a native UI core.
**Use its supported interfaces rather than inventing a Rust-only PocketJS API.**
Rust owns privileged or latency-critical services. MoonBit/WAMR and Lua are
optional components, not replacements for the PocketJS application contract.
Record a reproducible runtime/ABI compatibility spike before selecting a
specific host or pinning upstream revisions.

## Invariants — every phase

1. Games keep GPU and CPU resources whenever the shell is idle. No default
   periodic full-screen redraw or polling if nothing changes.
2. One graphics owner for a native session: connector/CRTC/plane lifetime,
   vblank, buffer import, page flips, input focus and suspend/resume are explicit.
3. One input policy for game vs. shell. Overlay visibility must not terminate a
   game; input suppression must be proven to avoid double-activating a game.
4. Game launch/stop/restart, controller hotplug, crash and suspend/resume must
   be bounded, safe and recoverable. No UI-supplied arbitrary privileged shell.
5. Low/Mid/High hardware tiers are *validation targets*, not claims of support:
   H700-class RG35XX H/RG40XX H; QCS6125-class device; Retroid Pocket 6.
   Keep board GPU/driver and licensing constraints explicit.
6. Preserve kernel/Mesa/SDL/Proton/Libretro compatibility. Prefer existing
   working upstream stacks over reimplementing emulation/graphics drivers.
7. CI software headless ≠ DRM scanout ≠ UTM VirtIO ≠ real handheld. Never
   upgrade evidence categories without matching test artifacts.

## Phase 1 — **Linux-native game session / Linux 原生图形会话**
**ACTIVE. Must work without KDE/KWin or other nested host.**

| Slice | Engineering deliverable | Pass/fail gate |
|---|---|---|
| P1-N01 | Read-only KMS probe and device/driver/connector capability inventory | Kernel ioctls, offline tests; output names actual mode/driver |
| P1-N02 | Authorized non-root libseat device lifecycle + udev/libinput events | TTY seat activate/pause/resume/hotplug; no KDE takeovers |
| P1-N03 | DRM connector/encoder/CRTC/plane selection; support failures and rollback | Fixtures and correct handling of absent/removed output |
| P1-N04 | GBM/EGL/GLES render target + atomic KMS modeset and page flip | Physical/VM TTY shows test frame; vblank cadence recorded; returns original VT |
| P1-N05 | Share XDG/layer-shell rendering and surface life-cycle core with native backend | SuperTux and system overlay render on *same native output* |
| P1-N06 | libinput seat focus (pointer/keyboard/touch), joystick/gamepad isolation | Input goes to either game OR system overlay as designed |
| P1-N07 | Native session service, crash/restart, monitor hotplug, suspend/resume | Quit daemon ≠ compositor quit; session cleanup and restore proven |

**Definition of Done P1:** in an authorized Linux TTY, without KDE,
a normal-user session starts the native compositor, SuperTux renders directly
through KMS, the menu draws above it, overlay input does not leak into the
game, hotplug/VT suspend and shutdown restore ownership safely. Confirm in
UTM where possible **and** on one physical handheld before claiming native
hardware support. CI uses mocks/headless protocol; cannot prove scanout.

Current evidence (2026-11-21): Nested Smithay + SuperTux + text menu accepted
by user in KDE/UTM. `rust/apps/pokdrm` probes KMS read-only; `rust/native`
opens authorized libseat/DRM, enumerates connector/CRTC and initializes
GBM/EGL, udev/libinput on x86-64/ARM64 CI. UTM read-only KMS preflight accepted (virtio-pci card0, Virtual-1, one CRTC/connector/encoder). UTM native scanout now user-observed (1280×800@120Hz, 2 DRM VBlank acknowledgements and final green frame); updated diagnostic holds both blue and green frames separately. **Native Wayland game presentation and physical handheld scanout remain unverified.** No measured power benefit.

## Phase 2 — **PocketJS-first system shell / 系统交互平台**
**Infrastructure spikes may run in parallel with P1; full integration depends
on the native surface and input APIs.**

| Slice | Deliverable | Pass/fail gate |
|---|---|---|
| P2-J01 | PocketJS source/version/license/API spike, host ABI + rendering contract | Build a minimal real PocketJS UI, measured binary/RAM, no browser |
| P2-J02 | Home, game library, game-detail, settings and quick overlay in PocketJS | UI in native compositor; input and lifecycle audited |
| P2-J03 | Explicit low-privilege shell RPC: launch, save, power, storage, connectivity | Versioned commands, permissions, timeouts, tests |
| P2-J04 | Extensible plug-in & theme APIs and deterministic UI rendering | Crash isolation; no arbitrary root/Shell/RPC escape |
| P2-J05 | Low-wakeup event-driven reactive rendering | Compare CPU wakeups and GPU presents vs. P1 baseline |

**Definition of Done P2:** normal-user boot to PocketJS, navigate with
gamepad/touch, start/stop/resume native games, open overlay, suspend/wake,
and no traditional desktop process is required.

## Phase 3 — **Unified emulator core host / 模拟核心统一运行层**

| Slice | Deliverable | Pass/fail gate |
|---|---|---|
| P3-E01 | Versioned `EmulatorCore` ABI, lifecycle/capability model | Core discovery, initialization, teardown and error tests |
| P3-E02 | One proven Libretro-like core adapter | Run legally obtained/homebrew ROM, input/audio/video/states |
| P3-E03 | Replace duplicate emulator frontend ownership | Video buffer/texture to compositor, common input/audio/save |
| P3-E04 | Fast path based on GBM/dma-buf/Vulkan/GLES where supported | Measure copies, sync, frame pacing, power; fallback correct |
| P3-E05 | Core isolation, rollback, permissions, compatibility matrix | Crash in core must not kill shell or wipe saves |

**Definition of Done P3:** at least one emulator core runs without its
traditional frontend and has functioning gamepad, audio, video, saves,
overlay and suspend/resume. Never assume every emulator can be stripped to
a core or every graphics path is zero-copy. Keep compatibility fallback.

## Phase 4 — **Lean Linux image & handheld power / 精简系统与续航**

| Slice | Deliverable | Pass/fail gate |
|---|---|---|
| P4-O01 | Reproducible trimmed Linux image + rollback/recovery | Boot to shell on validated hardware, secure update path |
| P4-O02 | Device profiles, governor, GPU clocks and adaptive display pacing | Measure per-game FPS, 1% lows, input latency and power |
| P4-O03 | Suspend/resume, sleep/wake controller, storage/network profiles | Long-running stability + battery drain regression |
| P4-O04 | Compatibility, benchmarks and release criteria per device tier | Compare against supported baseline with repeatable methods |

**Definition of Done P4:** documented boot and recovery on each supported
device, measured idle/game/suspend power and battery runtime, no regressions
in frame times/input, and clear third-party license compliance.

## Measurement contract

At each gate capture device/kernel/driver/Mesa version, CPU/GPU clocks,
resolution/refresh, rendering backend, test/game version, test duration,
5-run median or confidence bounds where possible, peak/steady RSS,
CPU wakeups, present/frame-time histogram, dropped frames, input-to-photon
latency and *measured* mW/Wh (with method). Compare to the nearest known
working OS/kernel on the **same** hardware. CI-only metrics never establish
battery life or hardware support.

## Execution rules / Codex subagent contract

See [agent work packages](docs/agent-workstreams.md) and
[verification matrix](docs/verification.md). A task is complete only when
its acceptance gate and evidence class are recorded. Prefer small cohesive
commits that leave `main` buildable; new work is direct to `main` in this
project **by user instruction**, with x86-64/ARM64 CI as a mandatory guard.
No PR ceremony; no claims of hardware success from compilation.

P1-N05 integration: shared Wayland protocol/render core and experimental native
Wayland socket/GBM frame submission added; native runtime acceptance pending.

P1-N05 frame pacing: one-in-flight KMS buffer state machine, no repeated
nested renders when no surfaces commit. Compile/CI and physical idle-power
measurement are independent gates. Do not claim battery savings before
reproducible measurements.

### P1 interoperability bridge: ROCKNIX / Sway (2026-11-22)

An optional, explicit external Wayland compatibility backend allows
pokshell's Rust game supervisor and SHM layer menu to run under
pre-existing ROCKNIX/Sway graphics/seat services. The bundled tool owns
no DRM master and never assumes it can stop the compositor. This
provides a near-term path for real games while the native Smithay
first-frame/SDL initialization remains unresolved. It must remain a
separate backend; replacing the target dedicated lightweight native
compositor or PocketJS UI with a general desktop is not accepted.
See `docs/verification.md` for opt-in syntax, isolation, CI and
license boundaries.

P1-ROCKNIX consolidation: canonical Rust `poksession --external-wayland`
replaces the Python session supervisor while retaining an exec-only
compatibility alias; the pre-existing Sway compositor remains the sole
DRM/seat owner. CI must keep the headless Sway SDL2/menu lifecycle test.

P2-J01 implementation (2026-11-22): original PocketJS Solid/TSX UI with
the pinned v0.17.0 product-linux-kms host contract. The software-headless
QuickJS/Rust admission test is CI-gated. A compositable Wayland host and
least-privilege command bridge remain required for P2-J02/03. Native
PocketJS hardware scanout is not inferred from headless execution.
