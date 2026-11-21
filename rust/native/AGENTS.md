# Native Linux graphics agent constraints

Inherited root contract: [AGENTS.md](../../AGENTS.md),
[ROADMAP.md](../../ROADMAP.md), [work packages](../../docs/agent-workstreams.md).

Work in `rust/native` on **P1 only** unless explicitly assigned elsewhere.
Native work requires DRM/KMS, libseat, GBM/EGL/GLES, hotplug, libinput and
safe VT suspend/resume. The nested Winit backend remains a regression harness,
not the product output. Native authority must come from the active seat,
not root or unrestricted `/dev/dri/card*` access.

Never treat discovering a connector, creating an EGL display or compiling a
renderer as evidence of a physical scanout. Tests must not modeset CI runners.
Every framebuffer lifecycle must define ownership, plane selection, vblank
sync, lost device, and teardown/restore behavior.

Changes to `rust/native/src/main.rs`, native Cargo features, workflow jobs
or cross-backend public APIs require lead integration; other modules can be
delegated via the work package table.
