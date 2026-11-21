#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Native poksession orchestration with a fake compositor + layer, real pokshelld.

No DRM access, no root and no graphical host. Verifies that the game is
spawned on the selected native Wayland display and the supervisor reaps all
three owned processes on shutdown. Run from rust/ after cargo build --workspace.
"""
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SESSION = ROOT / "target/debug/poksession"
DAEMON = ROOT / "target/debug/pokshelld"
DISPLAY = "pokshell-integration"

def wait(predicate, timeout=10):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if predicate():
            return True
        time.sleep(.04)
    return False

def component():
    role = Path(sys.argv[0]).name
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    record = runtime / (role + ".json")
    if role == "mock-compositor":
        expected = ["--wayland", "--card", "/dev/dri/card0"]
        if os.environ.get("POKSHELL_MOCK_SHM_ONLY") == "1":
            expected.append("--no-dmabuf")
        assert sys.argv[1:] == expected, (sys.argv, expected)
        assert "WAYLAND_SOCKET" not in os.environ
        endpoint = runtime / DISPLAY
        sock = socket.socket(socket.AF_UNIX)
        sock.bind(str(endpoint))
        sock.listen(5)
        sock.settimeout(.1)
        print("WAYLAND_DISPLAY=" + DISPLAY, flush=True)
        try:
            while True:
                try:
                    client, _ = sock.accept()
                    client.close()
                except socket.timeout:
                    pass
        finally:
            sock.close()
            endpoint.unlink(missing_ok=True)
    if role == "mock-layer":
        assert sys.argv[1:] == [
            "--menu", "--display", DISPLAY, "--socket", str(runtime / "pokshell.sock")
        ], sys.argv
        assert "WAYLAND_SOCKET" not in os.environ
        record.write_text(json.dumps({"role": role, "pid": os.getpid()}))
        while True:
            time.sleep(1)
    if role == "mock-game":
        assert os.environ.get("WAYLAND_DISPLAY") == DISPLAY, os.environ.get("WAYLAND_DISPLAY")
        assert os.environ.get("SDL_VIDEODRIVER") == "wayland"
        assert os.environ.get("XDG_SESSION_TYPE") == "wayland"
        assert "DISPLAY" not in os.environ and "WAYLAND_SOCKET" not in os.environ
        record.write_text(json.dumps({"role": role, "pid": os.getpid()}))
        while True:
            time.sleep(1)
    raise AssertionError(role)

def ipc(sock_path, cmd):
    with socket.socket(socket.AF_UNIX) as conn:
        conn.settimeout(3)
        conn.connect(str(sock_path))
        conn.sendall((cmd + "\n").encode())
        return conn.recv(1024).decode()

def test(no_dmabuf=False):
    with tempfile.TemporaryDirectory(prefix="pokshell-native-session-") as td:
        runtime = Path(td)
        for role in ["mock-compositor", "mock-layer", "mock-game"]:
            target = runtime / role
            target.write_text("#!" + sys.executable + "\n" +
                              Path(__file__).read_text().split("\n", 2)[2])
            target.chmod(0o700)
        env = os.environ.copy()
        for key in ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"]:
            env.pop(key, None)
        env.update({
            "XDG_RUNTIME_DIR": str(runtime),
            "XDG_SESSION_TYPE": "tty",
            "POKSHELL_MOCK_SHM_ONLY": "1" if no_dmabuf else "0",
        })
        args = [str(SESSION), "--native", "--card", "/dev/dri/card0",
                "--compositor", str(runtime / "mock-compositor"),
                "--daemon", str(DAEMON),
                "--layer", str(runtime / "mock-layer"),
                "--game", str(runtime / "mock-game")]
        if no_dmabuf:
            args.append("--no-dmabuf")
        output = runtime / "session.log"
        with output.open("wb") as writer:
            proc = subprocess.Popen(args, env=env, stdout=writer,
                                    stderr=subprocess.STDOUT)
        try:
            def ready():
                if proc.poll() is not None:
                    raise RuntimeError("session exited early:\n" + output.read_text())
                return ("POKSESSION_READY backend=native" in output.read_text() and
                        (runtime / "mock-game.json").exists() and
                        (runtime / "mock-layer.json").exists())
            assert wait(ready), output.read_text()
            assert ipc(runtime / "pokshell.sock", "STATUS").startswith("mode=game ")
            proc.send_signal(signal.SIGTERM)
            assert wait(lambda: proc.poll() is not None), output.read_text()
            assert proc.returncode == 0, output.read_text()
            assert not (runtime / "pokshell.sock").exists(), "daemon socket leaked"
            # Child processes should be stopped by the supervisor, not simply
            # left running after the parent has exited.
            for role in ["mock-game", "mock-layer"]:
                pid = json.loads((runtime / (role + ".json")).read_text())["pid"]
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                else:
                    raise AssertionError(f"{role} was not reaped: {pid}")
            print("PASS: native session owns compositor, game and menu")
            print("PASS: game is pinned to native Wayland; no KDE/X11 fallback")
            print("PASS: SIGTERM cleans children and daemon socket")
        finally:
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

if __name__ == "__main__":
    if Path(sys.argv[0]).name.startswith("mock-"):
        component()
    else:
        test(no_dmabuf=False)
        test(no_dmabuf=True)
