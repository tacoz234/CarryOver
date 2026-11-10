#!/usr/bin/env python3
"""Verify real setup pixels on the unlocked XWayland host, not just DOM presence."""
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

os.environ.setdefault("GDK_BACKEND", "x11")
import gi
gi.require_version("Gdk", "3.0")
gi.require_version("GdkX11", "3.0")
from gi.repository import Gdk, GdkX11, Gio

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
locked = bus.call_sync("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver",
                       "GetActive", None, None, Gio.DBusCallFlags.NONE, 2000, None).unpack()[0]
if locked:
    raise SystemExit("Unlock the KDE desktop before checking rendered pixels.")
binary = str(Path(sys.argv[1] if len(sys.argv)>1 else "target/release/copyblip").resolve())
with tempfile.TemporaryDirectory(prefix="carryover-rendering-") as root:
    env = dict(os.environ, HOME=root, XDG_CONFIG_HOME=root+"/config", XDG_DATA_HOME=root+"/data")
    # Do not mask the application's renderer selection with a test override.
    env.pop("WEBKIT_DISABLE_DMABUF_RENDERER", None)
    with open(root+"/app.log", "w+") as log:
        process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
        try:
            display = Gdk.Display.get_default()
            for _ in range(100):
                tree = subprocess.check_output(["xwininfo", "-root", "-tree"], text=True)
                ids = [line.split()[0] for line in tree.splitlines() if '"Set Up Carryover"' in line]
                for xid in ids:
                    owner = subprocess.run(["xprop", "-id", xid, "_NET_WM_PID"], capture_output=True, text=True).stdout
                    if re.search(r"=\s*"+str(process.pid)+r"\s*$", owner) is None:
                        continue
                    window = GdkX11.X11Window.foreign_new_for_display(display, int(xid,16))
                    width, height = window.get_width(), window.get_height()
                    pixels = Gdk.pixbuf_get_from_window(window,0,0,width,height)
                    if pixels is not None:
                        data=pixels.get_pixels();channels=pixels.get_n_channels()
                        colors=len(set(zip(*[iter(data)]*channels)))
                        if colors>100:
                            capture=os.environ.get("CARRYOVER_RENDER_CAPTURE")
                            if capture: pixels.savev(capture,"png",[],[])
                            print(f"PASS: setup paints real content ({width}x{height}, {colors} colors), without a renderer test override.")
                            break
                else:
                    time.sleep(.1)
                    continue
                break
            else:
                log.flush();log.seek(0);print(log.read())
                raise AssertionError("Setup DOM/window exists but its rendered content is blank")
        finally:
            process.terminate()
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.kill();process.wait()
