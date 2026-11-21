# Parallel engineering work packages (for Codex lead/subagents)

[Source of truth](../ROADMAP.md). **No conflicting writes to `main`.**
A lead agent queues integrations and owns `.github/workflows/ci.yml`,
`ROADMAP.md`, root `AGENTS.md`, `README.md`, release status and cross-crate
public interfaces. Each subagent owns scoped file paths in its worktree until
the lead reviews and merges the changes.

| Packet / priority | Agent role | Owned paths | Dependencies | Deliverable and deterministic tests |
|---|---|---|---|---|
| P1-N03a / immediate | DRM output analyst | `rust/native/src/kms.rs` | current libseat+KMS inventory | Pure connector/encoder/CRTC route planner; unit fixtures missing/duplicate route, multi-output, unplug |
| P1-N04a / immediate | Native renderer implementer | `rust/native/src/renderer.rs` | EGL/GBM initialization | Smithay GLES context + offscreen render target; compile/test without requesting DRM master |
| P1-N04b / next | KMS scanout integrator (**lead**) | `rust/native/src/main.rs`, `rust/native/src/output.rs` | N03a/N04a | Atomic modeset/pageflip/vblank, state restore and manual TTY evidence |
| P1-N02b / parallel | Seat/udev lifecycle analyst | `rust/native/src/seat.rs` | libseat notifier | Pause/resume gate + hotplug teardown state machine, pure tests |
| P1-N05 / later | Wayland backend architect | `rust/compositor/src/` (not main.rs until assigned) | native scanout, game surfaces | Shared XDG+layer compositor state, no duplicate IPC |
| P1-N06 / later | Input owner | `rust/native/src/input.rs` | P1-N05 | libinput focus arbitration, no game events while overlay active |
| P2-J01 / parallel spike | PocketJS integration researcher | `docs/pocketjs-integration.md` | pocket-nexus source/API/license | Reproducible build; native host/frame & input ABI, memory footprint; **no invented runtime** |
| P3-E01 / design spike | Emulator ABI researcher | `docs/emulator-core-abi.md` | Libretro interface evidence | Core adapter ABI, buffer/audio/input, states, failure isolation, licensing |
| P4-O01 / design spike | Linux image maintainer | `docs/minimal-linux.md` | P1 device requirements | Kernel/Mesa/seat/firmware/rootfs requirements, no early destructive flashing |
| CI / exclusive | Lead integrator | `.github/workflows/ci.yml`, root docs | all changes | x86-64 + ARM64, headless regression, no privileged device test |

## Package assignment contract

Specify: **task ID, objective, frozen interfaces, allowed files, forbidden
files, expected tests, failure/rollback behavior, evidence class**. Request
a patch/commit from each subagent, not prose-only suggestions.

Checklist for review: copyright/SPDX, target-specific deps, mocks for DRM
ioctls/event loops, disallow KDE takeover, error paths, allocation/sync,
ABI stability, CPU wakeups, power hypotheses vs measurements. Compare
Smithay API version to the checked-in dependency; never copy current
upstream examples blindly.

## Integration cadence

1. Lead checks `main` and states the user-facing acceptance criteria.
2. Agents research and implement in isolated worktrees. No direct concurrent
   pushes and no changes to other agent-owned files.
3. Lead reviews, rebases/serializes and tests each packet.
4. Push an atomic direct commit to `main`; watch all required CI jobs.
5. Fix red CI before claiming a pass. Update `docs/verification.md` and
   the relevant slice evidence in `ROADMAP.md`.
6. When physical/UTM access is needed, give the user exact **non-root**
   acceptance commands and expected diagnostics; don't claim success until
   they provide evidence.
