#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Nested Smithay + existing layer-shell client protocol integration.

Starts a real headless wlroots host (software renderer), then launches
pokshell's own compositor inside the host, and finally verifies that the
existing client receives configure from *pokshell*, not from Sway.
Does not assert visible pixels, games, DRM/KMS, or GPU acceleration.
"""
import os
import sys
import json
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
        fullscreen_mode = "--sdl2-fullscreen" in sys.argv
        sdl_mode = "--sdl2" in sys.argv or fullscreen_mode
        if sdl_mode:
            game_command = [
                "../../rust/target/debug/pokshelld",
                "--game", sys.executable,
                "--game-arg", "tests/sdl2_client.py",
            ]
            if fullscreen_mode:
                game_command.extend(["--game-arg", "--fullscreen"])
        else:
            game_command = [
                "../../rust/target/debug/pokshelld",
                "--game", "/usr/bin/weston-simple-shm",
            ]
        game_command.extend(["--game-wayland-display", "pokshell-test"])
        # Inject plausible KDE/XWayland variables into the daemon itself:
        # only the launched child may have these routes disabled.
        daemon_hostile = {
            **layer_env,
            "DISPLAY": ":77",
            "SDL_VIDEODRIVER": "x11",
        }
        daemon = spawn(game_command, daemon_hostile, daemon_log)
        wait_for(
            lambda: (runtime / "pokshell.sock").is_socket(),
            daemon, daemon_log, "pokshelld session daemon",
        )

        # Subscribe to actual game/overlay state. Never force --show.
        menu_mode = "--menu" in sys.argv
        layer_args = ["../../graphics/wayland-layer/target/debug/pokshell-layer",
                      "--menu" if menu_mode else "--interactive"]
        if not menu_mode:
            layer_args.append("--keyboard-interactive")
        # Always specify the *nested* server, including for the menu test.
        # Poison BOTH Wayland env variables: an env-only client would open
        # under the Sway/KDE host or fail on the invalid inherited fd.
        layer_args.extend(["--display", "pokshell-test"])
        layer_hostile_env = {
            **layer_env,
            "WAYLAND_DISPLAY": host_socket,
            "WAYLAND_SOCKET": "999999",
            "DISPLAY": ":77",
        }
        layer = spawn(
            layer_args,
            layer_hostile_env, layer_log,
        )
        wait_for(
            lambda: "pokshell-layer configured" in layer_log.read_text(errors="replace"),
            layer, layer_log, "pokshell layer client",
        )
        assert "connected to nested Wayland display=pokshell-test" in (
            layer_log.read_text(errors="replace")
        ), "client did not explicitly target the nested Wayland socket"
        assert "pokcomp: new layer_surface namespace=pokshell-system" in (
            comp_log.read_text(errors="replace")
        ), "the nested server did not receive the overlay surface"

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
        if sdl_mode:
            proof_path = runtime / "pokshell-sdl-proof.json"
            wait_for(
                proof_path.exists, daemon, daemon_log, "SDL2 Wayland proof",
            )
            proof = json.loads(proof_path.read_text())
            assert proof["driver"] == "wayland", proof
            assert proof["socket"] == "pokshell-test", proof
            assert proof["x11"] is None, proof
            assert proof["video"] == "wayland", proof
            assert proof["fullscreen_requested"] == fullscreen_mode, proof
            if fullscreen_mode:
                wait_for(
                    lambda: "pokcomp: xdg fullscreen size=" in
                    comp_log.read_text(errors="replace"),
                    compositor, comp_log, "XDG fullscreen configure",
                )
                assert proof["window_size"][0] > 0 and proof["window_size"][1] > 0, proof
        wait_for(
            lambda: "pokcomp: game render elements present" in comp_log.read_text(errors="replace"),
            compositor, comp_log, "nested game render elements",
        )
        first_status = control("STATUS")
        assert "mode=game" in first_status
        pid = next(part for part in first_status.split() if part.startswith("pid="))
        assert pid != "pid=none"

        assert "OK OVERLAY OPEN" in control("OVERLAY", "OPEN")
        wait_for(
            lambda: "pokcomp: layer render elements present" in comp_log.read_text(errors="replace"),
            compositor, comp_log, "nested layer render elements",
        )
        wait_for(
            lambda: "pokshell-layer buffer committed visible=1"
                in layer_log.read_text(errors="replace"),
            layer, layer_log, "visible diagnostic panel commit",
        )
        assert layer.poll() is None, "keyboard-enabled layer crashed"
        if menu_mode:
            assert "pokshell-layer buffer committed visible=1 selected=0" in (
                layer_log.read_text(errors="replace")
            ), "menu did not commit selected first row"
            wait_for(
                lambda: "pokcomp: exclusive system menu keyboard focus" in
                    comp_log.read_text(errors="replace"),
                compositor, comp_log, "menu exclusive keyboard focus",
            )
        assert "overlay=1" in control("STATUS")
        assert pid in control("STATUS"), "overlay opening replaced game PID"
        assert "OK OVERLAY CLOSED" in control("OVERLAY", "CLOSE")
        wait_for(
            lambda: "pokshell-layer buffer committed visible=0"
                in layer_log.read_text(errors="replace").split(
                    "pokshell-layer buffer committed visible=1", 1
                )[-1],
            layer, layer_log, "transparent panel commit after close",
        )
        assert pid in control("STATUS"), "overlay closing replaced game PID"
        if menu_mode:
            wait_for(
                lambda: "pokcomp: keyboard focus returned to game after layer hide"
                    in comp_log.read_text(errors="replace"),
                compositor, comp_log, "game keyboard focus restoration",
            )
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
