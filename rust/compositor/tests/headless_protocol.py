#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Test native-style Wayland eventfd dispatch and SDL2 init without DRM/KMS.

This is not a GPU test: pokwayland-protocol implements protocol dispatch only.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
HARNESS = ROOT / "wayland-core/target/debug/pokwayland-protocol"
CLIENT = ROOT / "compositor/tests/sdl2_client.py"

def until(predicate, alive, log, what, timeout=12):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if predicate():
            return
        if alive.poll() is not None:
            raise RuntimeError(f"{what}: process exited {alive.returncode}\n"
                               + log.read_text(errors="replace")[-6000:])
        time.sleep(0.05)
    raise RuntimeError(f"{what}: timeout\n" + log.read_text(errors="replace")[-6000:])

def stop(process):
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)

with tempfile.TemporaryDirectory(prefix="poknative-wayland-protocol-") as d:
    root = Path(d)
    runtime = root / "run"
    runtime.mkdir(mode=0o700)
    env = {**os.environ, "XDG_RUNTIME_DIR": str(runtime),
           "WAYLAND_DISPLAY": "pokshell-protocol", "SDL_VIDEODRIVER": "wayland"}
    for name in ("DISPLAY", "WAYLAND_SOCKET", "SDL_VIDEO_WAYLAND_ALLOW_LIBDECOR"):
        env.pop(name, None)
    server_log = root / "server.log"
    with server_log.open("wb") as output:
        server = subprocess.Popen([str(HARNESS)], env=env,
                                  stdout=output, stderr=subprocess.STDOUT)
    try:
        until(lambda: (runtime / "pokshell-protocol").is_socket(),
              server, server_log, "Wayland listener")
        for variant, override in [
            ("default", {}),
            ("no-libdecor", {"SDL_VIDEO_WAYLAND_ALLOW_LIBDECOR": "0"}),
        ]:
            client_log = root / f"client-{variant}.log"
            with client_log.open("wb") as output:
                client = subprocess.Popen([sys.executable, str(CLIENT), "--window-only"],
                                          env={**env, **override},
                                          stdout=output, stderr=subprocess.STDOUT)
            try:
                until(lambda: "pokshell-sdl: video_initialized" in
                      client_log.read_text(errors="replace"),
                      client, client_log, f"{variant} SDL2_VideoInit")
                client.wait(timeout=10)
                log = client_log.read_text(errors="replace")
                assert client.returncode == 0, log
                assert "pokshell-sdl: xdg_window_created window_only=1" in log, log
                until(lambda: "pokcomp: new xdg_toplevel" in
                      server_log.read_text(errors="replace"),
                      server, server_log, f"{variant} XDG role")
                until(lambda: "protocol_harness_xdg_toplevel_seen=yes" in
                      server_log.read_text(errors="replace"),
                      server, server_log, "request accounting")
                import re
                match = re.search(
                    r"protocol_harness_xdg_toplevel_seen=yes dispatched_requests=([0-9]+)",
                    server_log.read_text(errors="replace"),
                )
                assert match and int(match.group(1)) > 0, server_log.read_text()
                print(f"PASS: {variant} SDL init + XDG; dispatched={match.group(1)}")
            finally:
                stop(client)
        print("PASS: protocol-only regression ran without DRM/KMS/seat")
    finally:
        stop(server)
