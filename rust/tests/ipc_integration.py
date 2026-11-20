#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""End-to-end IPC tests, including event streaming and fragmented client writes."""
import os
from pathlib import Path
import socket as socket_api
import subprocess
import tempfile
import time

workspace = Path(__file__).resolve().parents[1]
bin_dir = Path(os.environ.get("POKSHELL_RUST_BIN", workspace / "target" / "debug"))

with tempfile.TemporaryDirectory(prefix="pokshell-") as directory:
    socket_path = Path(directory) / "session.sock"
    daemon = subprocess.Popen(
        [str(bin_dir / "pokshelld"), "--socket", str(socket_path),
         "--game", "/bin/sleep", "--game-arg", "120"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(100):
            if socket_path.exists():
                break
            if daemon.poll() is not None:
                raise AssertionError("pokshelld exited unexpectedly")
            time.sleep(0.02)
        assert socket_path.exists()
        assert socket_path.stat().st_mode & 0o777 == 0o600

        def call(*parts, expected_code=0):
            completed = subprocess.run(
                [str(bin_dir / "pokctl"), "--socket", str(socket_path), *parts],
                capture_output=True, text=True, timeout=6)
            assert completed.returncode == expected_code, completed
            return completed.stdout

        def connect():
            s = socket_api.socket(socket_api.AF_UNIX, socket_api.SOCK_STREAM)
            s.settimeout(4)
            s.connect(str(socket_path))
            return s

        assert call("PING") == "PONG\n"
        assert "mode=home" in call("STATUS")
        assert "ERROR" in call("OVERLAY", "OPEN", expected_code=1)

        # A request can arrive in multiple kernel reads. It must be framed by \n.
        with connect() as s:
            s.sendall(b"STA")
            time.sleep(0.03)
            s.sendall(b"TUS\n")
            assert b"mode=home" in s.recv(512)

        with connect() as s:
            s.sendall(b"x" * 260 + b"\n")
            assert s.recv(512).startswith(b"ERROR")

        # Persistent subscribers receive a snapshot followed by state events.
        watcher = connect()
        watcher.sendall(b"WATCH\n")
        messages = watcher.makefile("rb")
        assert messages.readline() == b"OK WATCH\n"
        assert b"mode=home" in messages.readline()

        assert "OK GAME START" in call("GAME", "START")
        assert b"type=game_started" in messages.readline()
        assert "mode=game" in call("STATUS")
        assert "OK OVERLAY OPEN" in call("OVERLAY", "OPEN")
        assert b"type=overlay_changed open=1" in messages.readline()
        assert "overlay=1" in call("STATUS")
        assert "OK OVERLAY CLOSED" in call("OVERLAY", "CLOSE")
        assert b"type=overlay_changed open=0" in messages.readline()
        assert "OK GAME STOP" in call("GAME", "STOP")
        exit_event = messages.readline()
        assert b"type=game_exited" in exit_event, exit_event
        assert b"signal=15" in exit_event, exit_event
        assert "mode=home" in call("STATUS")
        messages.close()
        watcher.close()

        assert "OK QUIT" in call("QUIT")
        assert daemon.wait(timeout=5) == 0
        assert not socket_path.exists()
    finally:
        if daemon.poll() is None:
            daemon.kill()
            daemon.wait(timeout=5)

print("PASS: Rust IPC, split messages, malformed inputs, event watch, SIGTERM stop, cleanup")
