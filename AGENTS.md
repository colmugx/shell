# Engineering rules

- Name: pokshell; game-console-first Linux Shell, not desktop UI.
- References: RG35XX H/RG40XX H (low), Retroid Duo Lite QCS6125 (mid), Retroid Pocket 6 (high).
- Never silently equate QEMU virt with handheld SoC or claim device compatibility without logs.
- Keep SDL/GLES native games separate from WAMR component workers. No WIT in phase one.
- PocketJS is the planned real native Shell; framebuffer demo is temporary and visually disposable.
- No new browser mock, no fake 'completed CFW' claims, no unverified screenshots.
- Next engineering gate: DRM/KMS + independent full-screen game + overlay; then PocketJS and WAMR.
- Use SPDX-License-Identifier: GPL-3.0-or-later for newly authored in-tree code where applicable. Preserve each upstream project's own license.
- Always run CMake build and tests; note anything untested on ARM64/UTM/hardware.
