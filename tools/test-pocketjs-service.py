#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Real PocketJS QuickJS service -> Rust pokshelld STATUS integration.

Never owns DRM/seat. Uses the real upstream headless host and our original
service provider; does not mock either the guest or the session daemon.
"""
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

root = Path(__file__).resolve().parents[1]
upstream = Path(sys.argv[1]).resolve()
host = upstream / "dist/pokshell-linux/runtime/pocket-linux-kms"
bundle = upstream / "apps/pokshell/dist/pokshell-home.pkb"
daemon_bin = root / "rust/target/debug/pokshelld"

with tempfile.TemporaryDirectory(prefix="pokshell-pocketjs-ipc-") as td:
    runtime = Path(td)
    ipc = runtime / "session.sock"
    env = dict(os.environ, XDG_RUNTIME_DIR=td, POKSHELL_SESSION_SOCKET=str(ipc))
    daemon = subprocess.Popen(
        [str(daemon_bin), "--socket", str(ipc),
         "--game", "/bin/sleep", "--game-arg", "30"],
        env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True,
    )
    try:
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline and not ipc.is_socket():
            if daemon.poll() is not None:
                raise AssertionError("pokshelld unexpectedly exited")
            time.sleep(0.03)
        assert ipc.is_socket(), "pokshelld failed to publish IPC"
        run = subprocess.run(
            [str(host), "--package", str(bundle), "--headless", "--frames", "20"],
            env=env, text=True, capture_output=True, timeout=15,
        )
        assert run.returncode == 0, (run.stdout, run.stderr)
        assert "completed 20 logical ticks" in run.stdout, run.stdout
        assert "pokshell-svc: action=STATUS accepted" in run.stderr, (
            "Guest did not call the real host service provider", run.stderr)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
            stream.settimeout(2)
            stream.connect(str(ipc))
            stream.sendall(b"STATUS\n")
            status = stream.recv(300).decode()
            assert "mode=home" in status, status
        print("PASS P2-J03: real PocketJS/QuickJS guest status intent reached Rust pokshelld")
        print("PASS P2-J03: no game started or compositor touched by the read-only probe")
    finally:
        if daemon.poll() is None:
            try:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
                    stream.settimeout(2)
                    stream.connect(str(ipc))
                    stream.sendall(b"QUIT\n")
                    stream.recv(300)
            except OSError:
                pass
            try:
                daemon.wait(timeout=5)
            except subprocess.TimeoutExpired:
                daemon.terminate()
                daemon.wait(timeout=5)
