# Run the Linux framebuffer alpha in an ARM Mac UTM VM

This is **not a full PocketJS console image**. It requires a Debian ARM64 QEMU/UTM VM with an exposed `/dev/fb0`. If `/dev/fb0` is unavailable, this specific alpha has no usable display path: the next architecture must use DRM/KMS.

## Start with Debian 13 ARM64 minimal

Choose UTM/QEMU AArch64 `virt`, a VirtIO GPU and keyboard, and 2-4 GB RAM. Native Debian inside UTM is the right VM base; board-specific Armbian belongs on real handheld hardware, not virtual QEMU `virt`.

```sh
uname -m
ls -l /dev/fb0
sudo apt update
sudo apt install -y cmake build-essential python3
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j
POCKET_BIN_DIR="$PWD/build" python3 tests/test_runtime.py
```

For graphical framebuffer mode, you can use `sudo ./build/pocket-session --game-bin "$PWD/build/pocket-game"`. In a separate SSH terminal, use `sudo ./build/pocketctl STATUS`; input is through evdev. If this fails, use headless rendering `./build/pocket-session --headless --frames 3 --screenshot /tmp/test.ppm`.

To install systemd autostart in a disposable test VM, inspect `tools/debian-install.sh` first. The scripts are not production hardened.

## Buildroot

The experimental `firmware/br-external` and `tools/build-buildroot.sh` build for **QEMU virt** on Linux, not for the three physical handhelds. The build scripts are in-tree, but image creation/boot has not been verified.

No ready-to-import UTM image is included in this repository.
