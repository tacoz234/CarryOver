#!/usr/bin/env python3
"""Optional actual-Plasma smoke check; temporary group, no clipboard writes."""
import ctypes
import ctypes.util
import os
import re
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/copyblip").resolve())
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def call(destination, path, interface, method, arguments=None):
    return bus.call_sync(destination, path, interface, method, arguments, None,
                         Gio.DBusCallFlags.NONE, 2000, None).unpack()


if call("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive")[0]:
    raise SystemExit("KDE desktop is locked. Unlock it before running this test; locked XWayland windows cannot be inspected reliably.")

with socket.socket() as connection:
    if connection.connect_ex(("127.0.0.1", 47821)) == 0:
        raise SystemExit("Another Carryover agent owns the control port; quit it before this isolated host check.")


def wait(check, description):
    for _ in range(100):
        if check():
            return
        time.sleep(0.1)
    raise AssertionError(description)


def mapped():
    result = subprocess.run(["xwininfo", "-name", "Carryover Panel"],
                            capture_output=True, text=True)
    return result.returncode == 0 and "Map State: IsViewable" in result.stdout


with tempfile.TemporaryDirectory(prefix="carryover-plasma-") as root:
    environment = dict(os.environ, HOME=root, XDG_CONFIG_HOME=root + "/config",
                       XDG_DATA_HOME=root + "/data", COPYBLIP_UPDATE_FEED="http://127.0.0.1:9")
    subprocess.run([binary, "init"], env=environment, stdout=subprocess.DEVNULL, check=True)
    with open(root + "/app.log", "w+") as log:
        process = subprocess.Popen([binary, "run", "--port", "0", "--tray", "--name", "Carryover Parity Check"],
                                   env=environment, stdout=log, stderr=log)
        service = f"org.kde.StatusNotifierItem-{process.pid}-1"
        try:
            def registered():
                items = call("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher",
                             "org.freedesktop.DBus.Properties", "Get",
                             GLib.Variant("(ss)", ("org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems")))[0]
                return any(service in item for item in items)
            wait(registered, "Plasma did not register the test tray item")
            geometry = subprocess.check_output(["xwininfo", "-root"], text=True)
            width = int(re.search(r"Width:\s+(\d+)", geometry).group(1))
            height = int(re.search(r"Height:\s+(\d+)", geometry).group(1))
            arguments = GLib.Variant("(ii)", (width - 160, height - 36))
            call(service, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Activate", arguments)
            wait(mapped, "Left-click popup did not map")

            def focused():
                active = subprocess.check_output(["xprop", "-root", "_NET_ACTIVE_WINDOW"], text=True).strip().split()[-1]
                return "Carryover Panel" in subprocess.check_output(["xprop", "-id", active, "_NET_WM_NAME"], text=True)
            wait(focused, "Popup didn't receive keyboard focus")
            # Send Escape only after confirming that our popup owns focus.
            x11 = ctypes.CDLL(ctypes.util.find_library("X11"))
            xtst = ctypes.CDLL(ctypes.util.find_library("Xtst"))
            x11.XOpenDisplay.restype = ctypes.c_void_p
            display = x11.XOpenDisplay(None)
            assert display, "Cannot connect to XWayland"
            x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
            x11.XKeysymToKeycode.restype = ctypes.c_ubyte
            xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
            x11.XFlush.argtypes = [ctypes.c_void_p]
            x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
            keycode = x11.XKeysymToKeycode(display, 0xFF1B)
            xtst.XTestFakeKeyEvent(display, keycode, 1, 0)
            xtst.XTestFakeKeyEvent(display, keycode, 0, 0)
            x11.XFlush(display)
            x11.XCloseDisplay(display)
            wait(lambda: not mapped(), "Native Escape did not dismiss the panel")
            time.sleep(0.5)
            call(service, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "ContextMenu", arguments)
            wait(mapped, "Right-click popup did not map")
            time.sleep(0.5)
            call(service, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Activate", arguments)
            wait(lambda: not mapped(), "Second activation didn't hide the popup")
            print("PASS: actual KDE Plasma/Bazzite registration, left/right popup, native Escape, toggle.")
        except Exception:
            log.flush()
            log.seek(0)
            print(log.read())
            raise
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
