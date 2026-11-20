# Engineering rules

- Name: pokshell. A game-first Linux system/session, not a desktop or themed launcher.
- Target hardware: RG35XX H/RG40XX H or H700 equivalent (Low), Retroid Duo Lite QCS6125 (Mid), RP6 (High).
- **Language order:** Rust first; MoonBit for portable system components where suitable; Lua as a light scripting option; TypeScript only when all suitable alternatives fail. C only for ABI boundaries and historical experiments.
- **Build path:** Debian ARM64 on UTM for generic testing; appropriate upstream Linux images on real handhelds; slim rootfs only after full native game session is working.
- New Rust code belongs in `rust/`. The old C framebuffer frontend is disposable; do not polish it into the product UI.
- Do not promise PocketJS without validating a viable Rust/MoonBit interface. No browser mock or Electron.
- Native ARM64 game processes are separate from WAMR Core Wasm component workers; no WIT in phase one.
- Plan for a native DRM/KMS/Wayland graphics session with game + independently interactive overlay. Changing an overlay flag does not meet this gate.
- No unverified UTM boot, hardware support or screenshot claims; CI build/test is **not** a physical device test.
- New in-tree original code: SPDX-License-Identifier: GPL-3.0-or-later. Preserve third-party licenses.
- Run `cargo test --workspace --locked`, `cargo build --workspace --locked`, and `python3 tests/ipc_integration.py` in `rust/`. Retain C baseline CI until deliberately retired.
