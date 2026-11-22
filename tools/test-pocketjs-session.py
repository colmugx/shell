#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""P2-J04: real PocketJS home in Rust poksession + pre-existing headless Sway.

The session must remain at Home before/after an independently launched game,
without taking over DRM/seat or stopping Sway. Real QuickJS, native pixels,
Wayland XDG and daemon IPC are exercised end-to-end.
"""
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = Path(sys.argv[1]).resolve()
HOST = UPSTREAM / "dist/pokshell-linux/runtime/pocket-linux-kms"
PACKAGE = UPSTREAM / "apps/pokshell/dist/pokshell-home.pkb"
SESSION = ROOT / "rust/target/debug/poksession"
LAYER = ROOT / "graphics/wayland-layer/target/debug/pokshell-layer"

def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=7)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=7)

def until(predicate, process, log, label, seconds=22):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if predicate():
            return
        if process.poll() is not None:
            raise AssertionError(f"{label} process exited: {process.returncode}\n"
                                 + log.read_text(errors="replace")[-7000:])
        time.sleep(.05)
    raise AssertionError(f"{label} not reached\n"+log.read_text(errors="replace")[-7000:])

def ipc(path, request):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(3)
        sock.connect(str(path))
        sock.sendall((request+"\n").encode())
        payload = bytearray()
        while not payload.endswith(b"\n") and len(payload) < 1024:
            chunk = sock.recv(1024-len(payload))
            if not chunk:
                break
            payload.extend(chunk)
        assert payload.endswith(b"\n"), f"truncated {request} response"
        return payload.decode()

with tempfile.TemporaryDirectory(prefix="pokshell-pocketjs-session-ci-") as td:
    root = Path(td)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "sway.conf"
    config.write_text("xwayland disable\noutput * resolution 1280x720\n"
                      "default_border none\n")
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
               WLR_RENDERER="pixman", WLR_HEADLESS_OUTPUTS="1",
               WLR_LIBINPUT_NO_DEVICES="1")
    for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY", "POKSHELL_SESSION_SOCKET"):
        env.pop(key, None)
    sway_log = root / "sway.log"
    session_log = root / "session.log"
    with sway_log.open("wb") as output:
        sway = subprocess.Popen(["sway", "-c", str(config), "--unsupported-gpu"],
                                env=env, stdout=output, stderr=subprocess.STDOUT)
    session = None
    try:
        def sockets():
            return sorted(p for p in runtime.glob("wayland-*") if p.is_socket())
        until(lambda: len(sockets()) == 1, sway, sway_log, "Sway socket")
        display = sockets()[0].name
        ipc_path = runtime / "pokshell.sock"
        with session_log.open("wb") as output:
            session = subprocess.Popen(
                [str(SESSION), "--external-wayland", display,
                 "--game", "/bin/sleep", "--game-arg", "30",
                 "--pocketjs", str(HOST), "--pocketjs-package", str(PACKAGE),
                 "--layer", str(LAYER)],
                env=env, stdout=output, stderr=subprocess.STDOUT)
        def content():
            return session_log.read_text(errors="replace")
        until(lambda: "POKSESSION_READY backend=external-sway" in content(),
              session, session_log, "PocketJS session ready")
        until(lambda: "pokshell-pocketjs-wayland: xdg_shm_buffer_committed" in content(),
              session, session_log, "PocketJS real Wayland frame")
        until(lambda: "pokshell-svc: action=STATUS accepted" in content(),
              session, session_log, "real native PocketJS status bridge")
        until(lambda: "pokshell-layer configured" in content(),
              session, session_log, "system overlay client")
        assert "ui=pocketjs" in content()
        status = ipc(ipc_path, "STATUS")
        assert "mode=home" in status and "pid=none" in status, status
        assert ipc(ipc_path, "GAME START").startswith("OK GAME START")
        until(lambda: "mode=game" in ipc(ipc_path, "STATUS"),
              session, session_log, "game startup")
        status = ipc(ipc_path, "STATUS")
        game_pid = next(p for p in status.split() if p.startswith("pid="))
        assert game_pid != "pid=none"
        assert ipc(ipc_path, "OVERLAY OPEN").startswith("OK OVERLAY OPEN")
        assert "pid=none" not in ipc(ipc_path, "STATUS")
        assert ipc(ipc_path, "OVERLAY CLOSE").startswith("OK OVERLAY CLOSED")
        assert game_pid in ipc(ipc_path, "STATUS"), "overlay restarted game"
        assert ipc(ipc_path, "GAME STOP").startswith("OK GAME STOP")
        until(lambda: "mode=home" in ipc(ipc_path, "STATUS"),
              session, session_log, "game returned Home")
        assert "pid=none" in ipc(ipc_path, "STATUS")
        assert session.poll() is None, "session quit when game stopped"
        assert sway.poll() is None, "PocketJS session incorrectly terminated Sway"
        session.send_signal(signal.SIGTERM)
        session.wait(timeout=12)
        assert session.returncode == 0, content()
        assert not ipc_path.exists(), "session IPC leaked"
        assert sway.poll() is None and (runtime/display).is_socket(), (
            "session stole or terminated external Sway")
        print("PASS P2-J04: PocketJS Home -> game -> overlay -> Home without session restart")
        print("PASS P2-J04: real PocketJS XDG pixels, service bridge and owned children")
        print("PASS P2-J04: external Sway compositor and its socket survive shutdown")
    finally:
        stop(session)
        stop(sway)
