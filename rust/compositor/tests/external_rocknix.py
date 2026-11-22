#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""ROCKNIX-style Sway-hosted game/menu integration, without DRM or root.

Sway is a CI-only headless dependency. This verifies that an externally
owned compositor remains alive after pokshell stops its owned children.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
RUNNER = ROOT / "rust/target/debug/poksession"
WRAPPER = ROOT / "tools/rocknix-session"
CLIENT = ROOT / "rust/compositor/tests/sdl2_client.py"
POKCTL = ROOT / "rust/target/debug/pokctl"


def spawn(argv, env, log):
    with log.open("wb") as output:
        return subprocess.Popen(argv, env=env, stdout=output, stderr=subprocess.STDOUT)


def wait_for(predicate, proc, log, label, seconds=18):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        if proc.poll() is not None:
            raise AssertionError(
                f"{label} exited {proc.returncode}:\n" + log.read_text(errors="replace")[-7000:]
            )
        time.sleep(0.05)
    raise AssertionError(f"{label} timeout:\n" + log.read_text(errors="replace")[-7000:])


def terminate(proc):
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


with tempfile.TemporaryDirectory(prefix="pokshell-rocknix-ci-") as directory:
    root = Path(directory)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "sway.conf"
    config.write_text("xwayland disable\noutput * resolution 1280x720\n"
                      "default_border none\n")
    sway_env = dict(os.environ)
    sway_env.update(XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
                    WLR_RENDERER="pixman", WLR_HEADLESS_OUTPUTS="1",
                    WLR_LIBINPUT_NO_DEVICES="1")
    sway_env.pop("WAYLAND_DISPLAY", None)
    sway_env.pop("WAYLAND_SOCKET", None)
    sway_env.pop("DISPLAY", None)
    sway_log = root / "sway.log"
    session_log = root / "session.log"
    sway = spawn(["sway", "-c", str(config), "--unsupported-gpu"], sway_env, sway_log)
    session = None
    try:
        def sockets():
            return sorted(p.name for p in runtime.glob("wayland-*") if p.is_socket())
        wait_for(lambda: bool(sockets()), sway, sway_log, "external Sway")
        display = sockets()[0]
        assert sway.poll() is None
        # Command-line compatibility remains available, but the Rust program
        # is now the only lifecycle owner.
        wrapper_help = subprocess.run([sys.executable, str(WRAPPER), "--help"],
                                     capture_output=True, text=True)
        assert wrapper_help.returncode == 0, wrapper_help.stderr
        # These inherited host variables must not escape to the game.
        hosted_env = dict(sway_env, WAYLAND_DISPLAY="wayland-nonexistent",
                          WAYLAND_SOCKET="999999", DISPLAY=":77")
        session = spawn(
            [str(RUNNER), "--external-wayland", display,
             "--game", sys.executable, "--game-arg", str(CLIENT)],
            hosted_env, session_log,
        )
        wait_for(
            lambda: "POKSESSION_READY backend=external-sway" in
            session_log.read_text(errors="replace"),
            session, session_log, "pokshell owned children")
        wait_for(
            lambda: "pokshell-layer configured" in session_log.read_text(errors="replace"),
            session, session_log, "external layer-shell configure")
        wait_for(
            lambda: "pokshell-sdl: shm_surface_submitted" in
            session_log.read_text(errors="replace"),
            session, session_log, "SDL2 window SHM frame")
        # P1-N06 protocol-focus contract: the game PID must stay alive while
        # the menu requests Exclusive on OPEN and None on CLOSE. No virtual
        # hardware keyboard is injected, so physical leakage stays unverified.
        def control(*parts):
            result = subprocess.run(
                [str(POKCTL), "--socket", str(runtime / "pokshell.sock"), *parts],
                env=hosted_env, capture_output=True, text=True, timeout=5,
            )
            assert result.returncode == 0, (result.args, result.stdout, result.stderr)
            return result.stdout

        status = control("STATUS")
        assert "mode=game" in status, status
        game_pid = next(p for p in status.split() if p.startswith("pid="))
        assert game_pid != "pid=none", status
        assert "OK OVERLAY OPEN" in control("OVERLAY", "OPEN")
        wait_for(
            lambda: "pokshell-layer keyboard_policy=exclusive overlay_visible=1" in
            session_log.read_text(errors="replace"),
            session, session_log, "visible menu exclusive focus request")
        wait_for(
            lambda: "pokshell-layer buffer committed visible=1" in
            session_log.read_text(errors="replace"),
            session, session_log, "menu visible frame")
        assert game_pid in control("STATUS"), "opening menu changed game PID"
        assert "overlay=1" in control("STATUS")
        assert "OK OVERLAY CLOSED" in control("OVERLAY", "CLOSE")
        wait_for(
            lambda: "pokshell-layer keyboard_policy=none overlay_visible=0" in
            session_log.read_text(errors="replace").split(
                "pokshell-layer keyboard_policy=exclusive overlay_visible=1", 1)[-1],
            session, session_log, "hidden menu releases keyboard request")
        wait_for(
            lambda: "pokshell-layer buffer committed visible=0" in
            session_log.read_text(errors="replace").split(
                "pokshell-layer buffer committed visible=1", 1)[-1],
            session, session_log, "hidden menu transparent frame")
        assert game_pid in control("STATUS"), "closing menu changed game PID"
        assert "overlay=0" in control("STATUS")
        session.send_signal(signal.SIGTERM)
        session.wait(timeout=8)
        assert session.returncode == 0, session_log.read_text()
        assert sway.poll() is None, "pokshell incorrectly terminated the external compositor"
        assert (runtime / display).is_socket(), "pokshell removed the external Wayland socket"
        assert not (runtime / "pokshell.sock").exists(), "pokshelld IPC socket leaked"
        print("PASS: existing Sway serves SDL2 game and pokshell layer shell")
        print("PASS: external Sway socket survives session termination")
        print("PASS: only pokshell-owned children/IPC are removed")
        print("PASS: overlay open/close toggles focus request and does not restart game")
    finally:
        terminate(session)
        terminate(sway)
