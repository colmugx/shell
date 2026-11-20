#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Linux-only end-to-end test of the Rust native session, no desktop required."""
import os
from pathlib import Path
import subprocess
import tempfile
import time

workspace = Path(__file__).resolve().parents[1]
bin_dir = Path(os.environ.get("POKSHELL_RUST_BIN", workspace / "target" / "debug"))

with tempfile.TemporaryDirectory(prefix="pokshell-") as directory:
    socket = Path(directory) / "session.sock"
    daemon = subprocess.Popen(
        [str(bin_dir / "pokshelld"), "--socket", str(socket),
         "--game", "/bin/sleep", "--game-arg", "120"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    try:
        for _ in range(100):
            if socket.exists():
                break
            if daemon.poll() is not None:
                raise RuntimeError("pokshelld exited before creating socket")
            time.sleep(0.02)
        assert socket.exists(), "session socket not created"
        assert socket.stat().st_mode & 0o777 == 0o600

        def call(*parts, expected_code=0):
            completed = subprocess.run(
                [str(bin_dir / "pokctl"), "--socket", str(socket), *parts],
                capture_output=True, text=True, timeout=4,
            )
            assert completed.returncode == expected_code, completed
            return completed.stdout

        assert call("PING") == "PONG\n"
        assert "mode=home" in call("STATUS")
        assert "ERROR" in call("OVERLAY", "OPEN", expected_code=1)
        assert "OK GAME START" in call("GAME", "START")
        assert "mode=game" in call("STATUS")
        assert "OK OVERLAY OPEN" in call("OVERLAY", "OPEN")
        assert "overlay=1" in call("STATUS")
        assert "OK OVERLAY CLOSED" in call("OVERLAY", "CLOSE")
        assert "OK GAME STOP" in call("GAME", "STOP")
        assert "mode=home" in call("STATUS")
        assert "OK QUIT" in call("QUIT")
        assert daemon.wait(timeout=4) == 0
        assert not socket.exists()
    finally:
        if daemon.poll() is None:
            daemon.kill()
            daemon.wait(timeout=4)
print("PASS: Rust Linux IPC, configured game child, overlay state, socket permissions")
