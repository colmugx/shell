#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Nested Smithay + existing layer-shell client protocol integration.

Starts a real headless wlroots host (software renderer), then launches
pokshell's own compositor inside the host, and finally verifies that the
existing client receives configure from *pokshell*, not from Sway.
Does not assert visible pixels, games, DRM/KMS, or GPU acceleration.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import time


def wait_for(predicate, process, log_path, label, timeout=20):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        value = predicate()
        if value:
            return value
        if process.poll() is not None:
            raise RuntimeError(
                f"{label} exited ({process.returncode}):\n"
                + log_path.read_text(errors="replace")[-9000:]
            )
        time.sleep(0.1)
    raise RuntimeError(
        f"{label} timeout:\n" + log_path.read_text(errors="replace")[-9000:]
    )


def stop(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=4)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=4)


def spawn(argv, env, log_path):
    stream = log_path.open("wb")
    try:
        return subprocess.Popen(
            argv, env=env, stdout=stream, stderr=subprocess.STDOUT
        )
    finally:
        stream.close()


with tempfile.TemporaryDirectory(prefix="pokcomp-nested-") as dirname:
    root = Path(dirname)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    conf = root / "sway.conf"
    conf.write_text(
        "xwayland disable\n"
        "output * resolution 1280x720\n"
        "font pango:monospace 10\n"
    )
    env = {
        **os.environ,
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "WLR_RENDERER": "pixman",
        "WLR_HEADLESS_OUTPUTS": "1",
        "LIBGL_ALWAYS_SOFTWARE": "1",
        "WGPU_BACKEND": "gl",
    }
    sway_log = root / "sway.log"
    comp_log = root / "pokcomp.log"
    layer_log = root / "layer.log"
    daemon_log = root / "daemon.log"
    daemon_env = None
    sway = spawn(["sway", "-c", str(conf), "--unsupported-gpu"], env, sway_log)
    compositor = None
    daemon = None
    layer = None
    try:
        host_socket = wait_for(
            lambda: next(
                (p.name for p in runtime.glob("wayland-*") if p.is_socket()),
                None,
            ),
            sway, sway_log, "headless host Sway",
        )
        nested_env = {
            **env,
            "WAYLAND_DISPLAY": host_socket,
            "WINIT_UNIX_BACKEND": "wayland",
        }
        compositor = spawn(
            ["./target/debug/pokcomp", "--socket", "pokshell-test"],
            nested_env, comp_log,
        )
        wait_for(
            lambda: (runtime / "pokshell-test").is_socket(),
            compositor, comp_log, "nested pokcomp",
        )
        assert compositor.poll() is None
        layer_env = {**nested_env, "WAYLAND_DISPLAY": "pokshell-test"}
        daemon = spawn(
            ["../../rust/target/debug/pokshelld", "--game", "/usr/bin/weston-simple-shm"],
            layer_env, daemon_log,
        )
        wait_for(
            lambda: (runtime / "pokshell.sock").is_socket(),
            daemon, daemon_log, "pokshelld session daemon",
        )

        # Subscribe to actual game/overlay state. Never force --show.
        layer = spawn(
            ["../../graphics/wayland-layer/target/debug/pokshell-layer"],
            layer_env, layer_log,
        )
        wait_for(
            lambda: "pokshell-layer configured" in layer_log.read_text(errors="replace"),
            layer, layer_log, "pokshell layer client",
        )

        def control(*args):
            result = subprocess.run(
                ["../../rust/target/debug/pokctl", *args],
                env=layer_env, capture_output=True, text=True, timeout=8,
            )
            if result.returncode:
                raise RuntimeError(
                    f"pokctl {args} failed: {result.stdout} {result.stderr}"
                )
            return result.stdout

        assert "OK GAME START" in control("GAME", "START")
        wait_for(
            lambda: "pokcomp: game render elements present" in comp_log.read_text(errors="replace"),
            compositor, comp_log, "nested game render elements",
        )
        assert "mode=game" in control("STATUS")

        assert "OK OVERLAY OPEN" in control("OVERLAY", "OPEN")
        wait_for(
            lambda: "pokcomp: layer render elements present" in comp_log.read_text(errors="replace"),
            compositor, comp_log, "nested layer render elements",
        )
        assert "overlay=1" in control("STATUS")
        assert "mode=game" in control("STATUS")
        assert "OK OVERLAY CLOSED" in control("OVERLAY", "CLOSE")
        assert "mode=game" in control("STATUS")
        assert "OK GAME STOP" in control("GAME", "STOP")
        assert "mode=home" in control("STATUS")
        assert "OK QUIT" in control("QUIT")
        assert compositor.poll() is None, comp_log.read_text(errors="replace")[-9000:]
        assert layer.poll() is None
        print("PASS: native XDG window and independent overlay coexist on pokcomp")
        print("PASS: game process survives overlay open/close")
    finally:
        stop(layer)
        stop(daemon)
        stop(compositor)
        stop(sway)
