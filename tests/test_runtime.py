#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import os, subprocess, tempfile, time
root=Path(__file__).resolve().parents[1]
bin=Path(os.environ.get("POCKET_BIN_DIR",root/"build"))
with tempfile.TemporaryDirectory() as d:
 sock=str(Path(d)/"ctl.sock")
 p=subprocess.Popen([str(bin/"pocket-session"),"--headless","--serve","--game-bin",str(bin/"pocket-game"),"--socket",sock])
 try:
  for _ in range(100):
   if Path(sock).exists():break
   if p.poll() is not None:raise AssertionError("session exited")
   time.sleep(.02)
  assert Path(sock).exists()
  def call(*args):
   r=subprocess.run([str(bin/"pocketctl"),*args],env={**os.environ,"POCKET_SOCKET":sock},capture_output=True,text=True,timeout=5)
   assert r.returncode==0,(r.stdout,r.stderr)
   return r.stdout
  assert "mode=home" in call("STATUS")
  assert "OK GAME START" in call("GAME","START")
  time.sleep(.12)
  assert "mode=game" in call("STATUS")
  assert "OK OVERLAY OPEN" in call("OVERLAY","OPEN")
  assert "overlay=1" in call("STATUS")
  assert "OK OVERLAY CLOSED" in call("OVERLAY","CLOSE")
  assert "OK GAME STOP" in call("GAME","STOP")
  assert "mode=home" in call("STATUS")
  assert "OK QUIT" in call("QUIT")
  assert p.wait(timeout=5)==0
 finally:
  if p.poll() is None:p.terminate();p.wait(timeout=5)
 shot=Path(d)/"shot.ppm"
 r=subprocess.run([str(bin/"pocket-session"),"--headless","--frames","2","--screenshot",str(shot)],timeout=5)
 assert r.returncode==0
 assert shot.read_bytes().startswith(b"P6\n480 270\n255\n")
print("PASS: Linux process, IPC, overlay state, headless render")
