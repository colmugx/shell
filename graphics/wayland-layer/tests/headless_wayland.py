#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Protocol smoke test on a real headless wlroots compositor.
Validates Wayland layer configure, not hardware GPU rendering or UTM.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import time

with tempfile.TemporaryDirectory(prefix="pokshell-wayland-") as dirname:
    root = Path(dirname)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    conf = root / "sway.conf"
    conf.write_text("xwayland disable\noutput * resolution 1280x720\nfont pango:monospace 10\n")
    env = {
        **os.environ,
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "WLR_RENDERER": "pixman",
        "WLR_HEADLESS_OUTPUTS": "1",
    }
    sway_log = root / "sway.log"
    layer_log = root / "layer.log"
    with sway_log.open("wb") as log:
        compositor = subprocess.Popen(
            ["sway", "-c", str(conf), "--unsupported-gpu"],
            env=env, stdout=log, stderr=subprocess.STDOUT,
        )
    layer = None
    try:
        deadline = time.monotonic() + 12
        path = None
        while time.monotonic() < deadline:
            sockets = [p for p in runtime.glob("wayland-*") if p.is_socket()]
            if sockets:
                path = sockets[0]
                break
            if compositor.poll() is not None:
                raise RuntimeError("headless sway exited:\n" + sway_log.read_text(errors="replace")[-6000:])
            time.sleep(.1)
        else:
            raise RuntimeError("Wayland socket not created:\n" + sway_log.read_text(errors="replace")[-6000:])
        env["WAYLAND_DISPLAY"] = path.name
        with layer_log.open("wb") as log:
            layer = subprocess.Popen(
                ["./target/debug/pokshell-layer", "--show"],
                env=env, stdout=log, stderr=subprocess.STDOUT,
            )
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if "pokshell-layer configured" in layer_log.read_text(errors="replace"):
                break
            if layer.poll() is not None:
                raise RuntimeError("layer client exited:\n" + layer_log.read_text(errors="replace"))
            time.sleep(.1)
        else:
            raise RuntimeError("no layer configure event:\n" + layer_log.read_text(errors="replace") +
                               "\nsway log:\n" + sway_log.read_text(errors="replace")[-6000:])
        assert layer.poll() is None
        assert compositor.poll() is None
        print("PASS: headless wlroots compositor configured an independent Wayland layer")
    finally:
        if layer and layer.poll() is None:
            layer.terminate()
            try: layer.wait(timeout=3)
            except subprocess.TimeoutExpired:
                layer.kill()
                layer.wait(timeout=3)
        if compositor.poll() is None:
            compositor.terminate()
            try: compositor.wait(timeout=3)
            except subprocess.TimeoutExpired:
                compositor.kill()
                compositor.wait(timeout=3)
