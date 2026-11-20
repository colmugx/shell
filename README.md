# pokshell

Game-first Linux console shell research project. Work in progress.

**Three reference tiers**: H700 handhelds (RG35XX H / RG40XX H or equivalent), Retroid Pocket Duo Lite, and Retroid Pocket 6. See [hardware baselines](docs/hardware-baselines.md).

## Direction

- A proper console session rather than a general-purpose desktop.
- Native ARM64 games get display/input/CPU/GPU priority.
- PocketJS is the planned native shell framework. It is **not yet integrated**.
- WAMR Core WebAssembly is planned for system/overlay components, **not games**. No WIT in the first phase.
- Platform-dependent kernel, GPU and input integrations remain replaceable.
- ARM64 QEMU/UTM is the development target, **not a model of the actual hardware**.

## Current implementation

The **Linux framebuffer alpha** is a native C technical prototype. It runs a fullscreen menu on `/dev/fb0`, supervises a separate sample-game process, reads evdev input, and draws a software overlay over a shared-memory demo game. It is **not** yet a PocketJS-based UI, a DRM/KMS game compositor, or a boot-tested hardware firmware.

**Linux build and test** (once the `src/`, `include/`, and `tests/` files are present):

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
POCKET_BIN_DIR="$PWD/build" python3 tests/test_runtime.py
./build/pocket-session --headless --frames 3 --screenshot /tmp/pokshell.ppm
```

The Debian/UTM integration is documented in [docs/utm.md](docs/utm.md). Buildroot recipes are experimental and have not been boot-verified. Do not flash the QEMU root filesystem onto a handheld.

## Governance

See [architecture](docs/architecture.md), [status and test matrix](docs/verification.md), and [licenses](docs/licensing.md). The project's own code is GPL-3.0-or-later. External components keep their original licenses.

**Project name, README and UI remain provisional.** No public ROMs, proprietary firmware, third-party game assets, or device keys are committed.
