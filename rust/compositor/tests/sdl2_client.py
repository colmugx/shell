#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""SDL2 ABI-level Wayland smoke client (test fixture, not the product UI).

Reproduces SDL's backend choice in a hostile KDE/X11 environment.
Default mode uses wl_shm via SDL_GetWindowSurface. --opengl independently
probes SDL2 OpenGL/EGL client initialization and Wayland buffer import.
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
sdl.SDL_SetWindowFullscreen.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
sdl.SDL_SetWindowFullscreen.restype = ctypes.c_int
sdl.SDL_PumpEvents.argtypes = []
sdl.SDL_GetWindowSize.argtypes = [
    ctypes.c_void_p, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int)
]
sdl.SDL_Quit.restype = None
sdl.SDL_GL_CreateContext.argtypes = [ctypes.c_void_p]
sdl.SDL_GL_CreateContext.restype = ctypes.c_void_p
sdl.SDL_GL_MakeCurrent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
sdl.SDL_GL_MakeCurrent.restype = ctypes.c_int
sdl.SDL_GL_SwapWindow.argtypes = [ctypes.c_void_p]
sdl.SDL_GL_DeleteContext.argtypes = [ctypes.c_void_p]

def check(code, operation):
    if code != 0:
        raise RuntimeError(f"{operation} failed: {sdl.SDL_GetError().decode()}")

if any(arg not in ("--fullscreen", "--opengl") for arg in sys.argv[1:]):
    raise SystemExit("usage: sdl2_client.py [--fullscreen] [--opengl]")
opengl = "--opengl" in sys.argv
check(sdl.SDL_Init(0x20), "SDL_Init(VIDEO)")
print(f"pokshell-sdl: video_initialized driver={sdl.SDL_GetCurrentVideoDriver()!r} backend={'opengl' if opengl else 'shm'}", flush=True)
try:
    driver = sdl.SDL_GetCurrentVideoDriver()
    assert driver == b"wayland", f"SDL selected {driver!r}, not nested Wayland"
    window = sdl.SDL_CreateWindow(b"pokshell SDL2 Wayland smoke", 50, 50, 640, 480,
                                  4 | (2 if opengl else 0))  # SHOWN | OPENGL
    if not window:
        raise RuntimeError("SDL_CreateWindow: " + sdl.SDL_GetError().decode())
    context = None
    try:
        print("pokshell-sdl: xdg_window_created", flush=True)
        if opengl:
            context = sdl.SDL_GL_CreateContext(window)
            if not context:
                raise RuntimeError("SDL_GL_CreateContext: " + sdl.SDL_GetError().decode())
            check(sdl.SDL_GL_MakeCurrent(window, context), "SDL_GL_MakeCurrent")
            print("pokshell-sdl: gl_context_ready", flush=True)
            gl = ctypes.CDLL("libGL.so.1")
            gl.glViewport.argtypes = [ctypes.c_int] * 4
            gl.glClearColor.argtypes = [ctypes.c_float] * 4
            gl.glClear.argtypes = [ctypes.c_uint]
            gl.glViewport(0, 0, 640, 480)
            gl.glClearColor(0.08, 0.4, 0.7, 1.0)
            gl.glClear(0x00004000)  # GL_COLOR_BUFFER_BIT
            sdl.SDL_GL_SwapWindow(window)
            print("pokshell-sdl: gl_swap_submitted", flush=True)
        else:
            surface = sdl.SDL_GetWindowSurface(window)
            if not surface:
                raise RuntimeError("SDL_GetWindowSurface: " + sdl.SDL_GetError().decode())
            check(sdl.SDL_FillRect(surface, None, 0x0055AA66), "SDL_FillRect")
            check(sdl.SDL_UpdateWindowSurface(window), "SDL_UpdateWindowSurface")
            print("pokshell-sdl: shm_surface_submitted", flush=True)
        if "--fullscreen" in sys.argv:
            # SDL_WINDOW_FULLSCREEN_DESKTOP (a real xdg_toplevel request)
            check(sdl.SDL_SetWindowFullscreen(window, 0x00001001),
                  "SDL_SetWindowFullscreen")
            for _ in range(100):
                sdl.SDL_PumpEvents()
                time.sleep(0.01)
        width = ctypes.c_int()
        height = ctypes.c_int()
        sdl.SDL_GetWindowSize(window, ctypes.byref(width), ctypes.byref(height))
        # Written only after SDL actually created and submitted the Wayland
        # window, not merely after opening a socket.
        result_path = os.path.join(os.environ["XDG_RUNTIME_DIR"], "pokshell-sdl-proof.json")
        with open(result_path, "w", encoding="utf-8") as output:
            json.dump({
                "driver": driver.decode(),
                "renderer": "opengl" if opengl else "shm",
                "socket": os.environ.get("WAYLAND_DISPLAY"),
                "x11": os.environ.get("DISPLAY"),
                "video": os.environ.get("SDL_VIDEODRIVER"),
                "fullscreen_requested": "--fullscreen" in sys.argv,
                "window_size": [width.value, height.value],
            }, output)
        # Pump Wayland events while holding the diagnostic image.
        for _ in range(200):
            sdl.SDL_PumpEvents()
            time.sleep(0.1)
    finally:
        if context:
            sdl.SDL_GL_DeleteContext(context)
        sdl.SDL_DestroyWindow(window)
finally:
    sdl.SDL_Quit()
