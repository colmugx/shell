#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Render the real pinned PocketJS app into a headless Sway wl_shm surface.

Checks actual XDG configure and wl_shm buffer attachment, not pixel
perception or real keyboard focus. Never takes DRM master or uses root.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

root = Path(__file__).resolve().parents[1]
upstream = Path(sys.argv[1]).resolve()
host = upstream / "dist/pokshell-linux/runtime/pocket-linux-kms"
bundle = upstream / "apps/pokshell/dist/pokshell-home.pkb"

with tempfile.TemporaryDirectory(prefix="pokshell-pocketjs-wayland-ci-") as td:
    path = Path(td)
    runtime = path / "runtime"
    runtime.mkdir(mode=0o700)
    config = path / "sway.conf"
    config.write_text("xwayland disable\noutput * resolution 1280x720\n"
                      "default_border none\n")
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime),
               WLR_BACKENDS="headless", WLR_RENDERER="pixman",
               WLR_HEADLESS_OUTPUTS="1", WLR_LIBINPUT_NO_DEVICES="1")
    for name in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
        env.pop(name, None)
    sway_log = path / "sway.log"
    with sway_log.open("wb") as out:
        sway = subprocess.Popen(["sway", "-c", str(config), "--unsupported-gpu"],
                                env=env, stdout=out, stderr=subprocess.STDOUT)
    try:
        limit = time.monotonic() + 15
        sockets = []
        while time.monotonic() < limit:
            if sway.poll() is not None:
                raise AssertionError("Headless Sway failed: " + sway_log.read_text(errors="replace")[-3000:])
            sockets = [p for p in runtime.glob("wayland-*") if p.is_socket()]
            if sockets:
                break
            time.sleep(0.05)
        assert len(sockets) == 1, f"one Sway Wayland socket required: {sockets}"
        display = sockets[0].name
        # Host must not accidentally use inherited display; the explicit
        # socket name selects our privately owned CI compositor.
        child = subprocess.run(
            [str(host), "--package", str(bundle), "--wayland-display", display,
             "--frames", "12"],
            env=env, text=True, capture_output=True, timeout=25)
        assert child.returncode == 0, (child.stdout, child.stderr)
        assert "xdg_configured" in child.stderr, child.stderr
        assert "xdg_shm_buffer_committed size=800x600" in child.stderr, child.stderr
        assert "completed 12 logical ticks" in child.stdout, child.stdout
        assert "external_compositor_untouched=yes" in child.stderr, child.stderr
        assert sway.poll() is None, "PocketJS terminated Sway"
        assert sockets[0].is_socket(), "PocketJS removed Sway's socket"
        print("PASS P2-J02: real PocketJS QuickJS/Rust UI submits wl_shm image into Sway")
        print("PASS P2-J02: external Sway survives and keeps its Wayland socket")
    finally:
        if sway.poll() is None:
            sway.terminate()
        try:
            sway.wait(timeout=5)
        except subprocess.TimeoutExpired:
            sway.kill()
            sway.wait(timeout=5)
