#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Unverified recipe; requires Buildroot source and network.
set -euo pipefail
: "${BUILDROOT_DIR:?Set BUILDROOT_DIR to a compatible Buildroot tree}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
make -C "$BUILDROOT_DIR" BR2_EXTERNAL="$ROOT/firmware/br-external" pokshell_qemu_aarch64_defconfig
make -C "$BUILDROOT_DIR" -j"$(nproc)"
