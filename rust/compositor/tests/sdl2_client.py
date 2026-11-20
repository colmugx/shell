#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""SDL2 ABI-level Wayland smoke client (test fixture, not the product UI).

Reproduces SDL's backend choice in a hostile KDE/X11 environment.
Uses wl_shm via SDL_GetWindowSurface, intentionally NOT OpenGL/dmabuf.
"""
import ctypes
import json
import os
import sys
import time

# Debian/Ubuntu provide this stable SDL2 ABI soname even when the
# development package (and its linker metadata) is not installed.
try:
    sdl = ctypes.CDLL("libSDL2-2.0.so.0")
except OSError as err:
    raise SystemExit(f"SDL2 runtime library unavailable: {err}") from err
sdl.SDL_Init.argtypes = [ctypes.c_uint32]
sdl.SDL_Init.restype = ctypes.c_int
sdl.SDL_GetError.restype = ctypes.c_char_p
sdl.SDL_GetCurrentVideoDriver.restype = ctypes.c_char_p
sdl.SDL_CreateWindow.argtypes = [
    ctypes.c_char_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_uint32,
]
sdl.SDL_CreateWindow.restype = ctypes.c_void_p
sdl.SDL_GetWindowSurface.argtypes = [ctypes.c_void_p]
sdl.SDL_GetWindowSurface.restype = ctypes.c_void_p
sdl.SDL_FillRect.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32]
sdl.SDL_FillRect.restype = ctypes.c_int
sdl.SDL_UpdateWindowSurface.argtypes = [ctypes.c_void_p]
sdl.SDL_UpdateWindowSurface.restype = ctypes.c_int
sdl.SDL_DestroyWindow.argtypes = [ctypes.c_void_p]
sdl.SDL_Quit.restype = None

def check(code, operation):
    if code != 0:
        raise RuntimeError(f"{operation} failed: {sdl.SDL_GetError().decode()}")

check(sdl.SDL_Init(0x20), "SDL_Init(VIDEO)")
try:
    driver = sdl.SDL_GetCurrentVideoDriver()
    assert driver == b"wayland", f"SDL selected {driver!r}, not nested Wayland"
    window = sdl.SDL_CreateWindow(b"pokshell SDL2 Wayland smoke", 50, 50, 640, 480, 4)
    if not window:
        raise RuntimeError("SDL_CreateWindow: " + sdl.SDL_GetError().decode())
    try:
        surface = sdl.SDL_GetWindowSurface(window)
        if not surface:
            raise RuntimeError("SDL_GetWindowSurface: " + sdl.SDL_GetError().decode())
        check(sdl.SDL_FillRect(surface, None, 0x0055AA66), "SDL_FillRect")
        check(sdl.SDL_UpdateWindowSurface(window), "SDL_UpdateWindowSurface")
        # Written only after SDL actually created and submitted the Wayland
        # window, not merely after opening a socket.
        result_path = os.path.join(os.environ["XDG_RUNTIME_DIR"], "pokshell-sdl-proof.json")
        with open(result_path, "w", encoding="utf-8") as output:
            json.dump({
                "driver": driver.decode(),
                "socket": os.environ.get("WAYLAND_DISPLAY"),
                "x11": os.environ.get("DISPLAY"),
                "video": os.environ.get("SDL_VIDEODRIVER"),
            }, output)
        time.sleep(20)
    finally:
        sdl.SDL_DestroyWindow(window)
finally:
    sdl.SDL_Quit()
