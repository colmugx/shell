#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression: the supervised game must use the nested Wayland socket.

Unlike the compositor smoke test, deliberately injects conflicting KDE
DISPLAY, host WAYLAND_DISPLAY, SDL_VIDEODRIVER=x11 and WAYLAND_SOCKET;
checks that the real child process cannot inherit those routes.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

BIN = Path(__file__).resolve().parents[1] / "target" / "debug"
GAME_SCRIPT = """#!/usr/bin/env python3
import json, os, socket, sys, time
runtime = os.environ["XDG_RUNTIME_DIR"]
display = os.environ["WAYLAND_DISPLAY"]
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(os.path.join(runtime, display))
with open(sys.argv[1], "w", encoding="utf-8") as output:
    json.dump({key: os.environ.get(key) for key in (
        "WAYLAND_DISPLAY", "DISPLAY", "WAYLAND_SOCKET", "SDL_VIDEODRIVER",
        "GDK_BACKEND", "QT_QPA_PLATFORM", "XDG_SESSION_TYPE",
    )}, output)
time.sleep(25)
"""

def until(predicate, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.03)
    raise AssertionError("test timed out")

with tempfile.TemporaryDirectory(prefix="pokshell-wayland-isolation-") as directory:
    runtime = Path(directory)
    game = runtime / "record_game.py"
    output = runtime / "env.json"
    game.write_text(GAME_SCRIPT)
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(runtime / "pokshell-test"))
    listener.listen(4)
    listener.settimeout(5)
    session_socket = runtime / "session.sock"
    env = {
        **os.environ,
        "XDG_RUNTIME_DIR": str(runtime),
        "WAYLAND_DISPLAY": "wayland-host",
        "WAYLAND_SOCKET": "999",
        "DISPLAY": ":999",
        "SDL_VIDEODRIVER": "x11",
    }
    daemon = subprocess.Popen([
        str(BIN / "pokshelld"), "--socket", str(session_socket),
        "--game", sys.executable,
        "--game-arg", str(game),
        "--game-arg", str(output),
        "--game-wayland-display", "pokshell-test",
    ], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    try:
        until(session_socket.is_socket)
        def command(*args):
            p = subprocess.run(
                [str(BIN / "pokctl"), "--socket", str(session_socket), *args],
                capture_output=True, text=True, timeout=5,
            )
            return p.returncode, p.stdout + p.stderr

        assert command("PING") == (0, "PONG\n")
        # Fail closed, don't accidentally attempt X11 when compositor
        # disappears between session creation and game start.
        listener.close()
        (runtime / "pokshell-test").unlink()
        code, result = command("GAME", "START")
        assert code != 0 and "nested game socket" in result, (code, result)
        assert "mode=home" in command("STATUS")[1]

        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(runtime / "pokshell-test"))
        listener.listen(4)
        listener.settimeout(5)
        code, result = command("GAME", "START")
        assert code == 0 and "OK GAME START pid=" in result, (code, result)
        client, _ = listener.accept()
        client.close()
        until(output.exists)
        child = json.loads(output.read_text())
        assert child["WAYLAND_DISPLAY"] == "pokshell-test", child
        assert child["SDL_VIDEODRIVER"] == "wayland", child
        assert child["DISPLAY"] is None, child
        assert child["WAYLAND_SOCKET"] is None, child
        assert child["GDK_BACKEND"] == "wayland", child
        assert child["QT_QPA_PLATFORM"] == "wayland", child
        assert child["XDG_SESSION_TYPE"] == "wayland", child
        assert "mode=game" in command("STATUS")[1]
        assert command("GAME", "STOP")[0] == 0
        assert command("QUIT") == (0, "OK QUIT\n")
        assert daemon.wait(timeout=5) == 0
        print("PASS: game process forced to nested Wayland, host X11/Wayland fallback removed")
        print("PASS: nested socket missing causes fail-closed GAME START")
    finally:
        if daemon.poll() is None:
            daemon.terminate()
            daemon.wait(timeout=5)
        listener.close()
