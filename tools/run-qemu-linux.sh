#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
OUT="${1:-output}"
test -f "$OUT/images/Image" && test -f "$OUT/images/rootfs.ext4" || { echo 'Missing Image/rootfs.ext4' >&2; exit 1; }
exec "${QEMU:-qemu-system-aarch64}" -machine virt -cpu cortex-a72 -m 2048 -smp 4 \
 -kernel "$OUT/images/Image" \
 -append 'root=/dev/vda rootwait console=ttyAMA0 loglevel=3' \
 -drive "file=$OUT/images/rootfs.ext4,format=raw,if=none,id=disk0" \
 -device virtio-blk-pci,drive=disk0 \
 -device virtio-gpu-pci -device virtio-keyboard-pci \
 -serial mon:stdio -display default -netdev user,id=net0 -device virtio-net-pci,netdev=net0
