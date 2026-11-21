# Codex / subagent operating contract — pokshell

Read [ROADMAP.md](ROADMAP.md), [docs/agent-workstreams.md](docs/agent-workstreams.md),
[docs/verification.md](docs/verification.md) **before coding**. Treat them as
product requirements, not optional suggestions.

## Mission
A next-generation lean **Linux handheld gaming OS**, not a KDE theme,
launcher, desktop clone or indefinitely nested Wayland demo.
**PocketJS** owns shell UI; **Rust** owns Linux graphics/seat/IPC/game services;
unified emulator *cores* reuse rendering/audio/input/saves instead of shipping
redundant emulator frontends. Minimal Linux and measured performance/battery
are outcome goals. Do not optimize tiny abstractions without a measurement.

## Priority and milestones
- **P1 ACTIVE:** native Linux DRM/KMS/GBM/EGL/GLES/seat, Wayland XDG/layer
  composition, vblank, input focus, VT suspend/resume.
- P2 PocketJS native UI integration spike can progress alongside P1; avoid
  inventing a PocketJS API. JavaScript/TypeScript is appropriate **inside
  PocketJS**, even if Rust remains the systems language. MoonBit/WAMR/Lua
  are opt-in where they measurably help.
- P3 Libretro/adapters and unified emulator core host.
- P4 lean rootfs, power/performance, hardware enablement.
- Never substitute a browser/Electron/KDE app as the product shell.
- Proof-of-concept/UTM nested compositor is a **test harness only**.

## Agent collaboration
The lead Codex agent owns cross-cutting architecture, sequencing, API review,
integration, CI and `main` serialization. Delegate only bounded work packages
with **id, allowed paths, dependencies, outputs and acceptance tests** as in
`docs/agent-workstreams.md`. Use separate worktrees or nonoverlapping file
ownership; agents do not blindly push over one another. Collect results,
inspect diffs, test, and integrate sequentially. Subagents must not change
product scope, public IPC/ABI, workflows or license policy independently.

**Repository user preference:** make incremental direct commits to `main`,
no PR process. Check the current SHA before updating. Build/test failures
are blockers, fix them before reporting a milestone as passing. Keep CI on
x86-64 and ARM64; maintain nested Wayland integration and old C baseline
until explicitly retired. Do not run privileged native DRM tests in CI.

## Required tests and evidence
- `cd rust && cargo test --workspace --locked && cargo build --workspace --locked && python3 tests/ipc_integration.py`
- `cargo test --manifest-path rust/native/Cargo.toml && cargo build --manifest-path rust/native/Cargo.toml` (Linux system headers/libseat/DRM/GBM/udev/libinput/EGL)
- `cargo test --manifest-path rust/compositor/Cargo.toml`
- `cargo test --manifest-path graphics/wayland-layer/Cargo.toml`
- GitHub Actions native x86-64/ARM64, headless/nested protocol tests.
- Manual tests only from authorized **non-root** Linux TTY when touching DRM
  master/modeset; no `sudo` compositor and no native takeover from KDE.
- Keep status `source-only`, `compile verified`, `headless protocol`,
  `UTM user accepted`, `physical device accepted` distinct. Never invent
  hardware timings, power/battery numbers or screen captures.

## Safety, licensing and boundaries
- GPL-3.0-or-later SPDX for original new code; preserve third-party notices.
- Never use `rm -rf` on user workspaces, overwrite a live Unix socket or
  flash images without device-specific instructions and user approval.
- Keep game executables separate processes by default; bounded IPC, user
  permissions, no shell command interpolation or arbitrary privileged UI RPC.
- DRM access must be seat-authorized, VT pause/resume must inhibit rendering,
  and output state must be restored safely before claiming P1 complete.
- Implement graceful fallback for missing connectors/GBM/EGL; handle
  disconnect, no devices and renderer errors explicitly.
- Do not expose proprietary console firmware or copyrighted game assets.

## Task finish template (commit message/notes)
`<slice ID>: code paths; checks+CI URL; evidence class; remaining gate`.
When a user requests continued development, perform actual code+tests; do not
send status-only responses. Only mark a slice finished when its tests pass.
