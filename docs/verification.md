# Verification status

All statements below describe the checked-in bootstrap and its independent CI, not completed hardware bring-up.

| Feature | Evidence | Status |
| --- | --- | --- |
| Linux x86_64 native CMake compilation | GitHub Actions build on Linux x86_64 | CI tested; compiler may report indentation warnings |
| Headless renderer screenshot and test game process | `tests/test_runtime.py` | Tested on Linux x86_64 |
| Local socket commands and overlay state | `tests/test_runtime.py` | Tested on Linux x86_64 |
| ARM64 cross-build | GitHub Actions native AArch64 runner (once job completes) | CI pending |
| Debian ARM64 under UTM | No current boot log | Not verified |
| Buildroot generated rootfs boot in QEMU virt | No current boot log | Not verified |
| PocketJS native shell | No linked PocketJS Linux host in this drop | Not implemented |
| WAMR component worker | Not in this framebuffer alpha | Not implemented |
| DRM/KMS hardware compositor / arbitrary SDL games | No implementation | Not implemented |
| RG35XX H, RG40XX H, Duo Lite, RP6 on-device support | No hardware tests | Not verified |

## CI interpretation

CI verifies that the source **compiles** and that the headless Linux test passes; it does not prove that the AArch64 QEMU image boots or that game compositing works on a physical handheld.

## Safety

Never flash an experimental QEMU image to any reference handheld. Do not run the current privileged framebuffer/input prototype as a network-exposed service.
