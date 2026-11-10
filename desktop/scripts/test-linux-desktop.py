#!/usr/bin/env python3
"""Real GTK/WebKit UI checks. Run in the isolated session from test-linux-desktop.sh."""
import os
import signal
import json
import http.server
import threading
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pyatspi
from gi.repository import Gio, GLib


def nodes(node=None, depth=0):
    if node is None:
        node = pyatspi.Registry.getDesktop(0)
    if depth > 18:
        return
    try:
        yield node
        for child in node:
            yield from nodes(child, depth + 1)
    except (GLib.Error, RuntimeError):
        return


def wait(check, description, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (json.JSONDecodeError, FileNotFoundError):
            # Polling can coincide with the app's in-progress file write.
            pass
        time.sleep(0.1)
    tree = [(n.getRoleName(), n.name) for n in nodes()]
    raise AssertionError(f"Timed out: {description}\n{tree}")


def find(name, role=None, partial=False):
    for n in nodes():
        try:
            if role is not None and n.getRoleName() != role:
                continue
            matched = name in n.name if partial else n.name == name
            if partial and not matched:
                try:
                    matched = name in n.queryText().getText(0, -1)
                except (NotImplementedError, GLib.Error):
                    pass
            if matched:
                return n
        except GLib.Error:
            pass


def click(name, role="push button", partial=False):
    n = wait(lambda: find(name, role, partial), name)
    action = n.queryAction()
    assert action.nActions and action.doAction(0), (name, "no action")
    return n


def control(command):
    with socket.create_connection(("127.0.0.1", 47821), timeout=2) as sock:
        sock.sendall((command + "\n").encode())
        assert sock.recv(100).startswith(b"ok")


if os.environ.get("CARRYOVER_DESKTOP_TEST") != "isolated":
    raise SystemExit("Use test-linux-desktop.sh inside an isolated test container.")

class InvalidFeed(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b'{"version":"99.0.0","files":{}}' if self.path.endswith(".json") else b"00" * 64
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass

feed = http.server.ThreadingHTTPServer(("127.0.0.1", 0), InvalidFeed)
threading.Thread(target=feed.serve_forever, daemon=True).start()
binary = str(Path(sys.argv[1]).resolve())
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def tray_service():
    names = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus",
        "org.freedesktop.DBus", "ListNames", None, None,
        Gio.DBusCallFlags.NONE, 2000, None).unpack()[0]
    return next((name for name in names if name.startswith("org.kde.StatusNotifierItem-")), None)


def activate(method="Activate"):
    time.sleep(0.45)  # Matches the Windows guard against reopening on a dismissal click.
    service = wait(tray_service, "StatusNotifierItem registration")
    bus.call_sync(service, "/StatusNotifierItem", "org.kde.StatusNotifierItem", method,
        GLib.Variant("(ii)", (700, 740)), None, Gio.DBusCallFlags.NONE, 2000, None)


with tempfile.TemporaryDirectory(prefix="carryover-desktop-") as tmp:
    root = Path(tmp)
    env = dict(os.environ, HOME=tmp, XDG_CONFIG_HOME=str(root / "config"),
               XDG_DATA_HOME=str(root / "data"),
               COPYBLIP_UPDATE_FEED=f"http://127.0.0.1:{feed.server_port}",
               CARRYOVER_OPEN_LOG=str(root / "opened-log"))
    (root / "bin").mkdir()
    opener = root / "bin/xdg-open"
    opener.write_text('#!/bin/sh\nprintf "%s\n" "$@" > "$CARRYOVER_OPEN_LOG"\n')
    opener.chmod(0o755)
    env["PATH"] = str(root / "bin") + ":" + env["PATH"]
    config = root / "custom profile/CopyBlip/config.json"
    installed = root / "data/Carryover/copyblip"
    login = root / "config/autostart/com.coledeterman.carryover.desktop"
    log = (root / "desktop.log").open("w+")
    process = subprocess.Popen([binary, "--config-dir", str(config.parent)], env=env, stdout=log, stderr=log)
    try:
        click("Continue")
        click("I already use Carryover", partial=True)
        entry = wait(lambda: next((n for n in nodes() if n.getRoleName() == "entry"), None), "invite entry")
        bounds = entry.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        subprocess.run(["xdotool", "mousemove", str(bounds.x + 10), str(bounds.y + 10), "click", "1", "type", "--clearmodifiers", "not-an-invite"], check=True)
        click("Install")
        wait(lambda: find("That isn't a Carryover invite", partial=True), "invalid invite error")
        assert not config.exists() and not installed.exists()
        click("This is my first device", partial=True)
        click("Install")
        wait(lambda: config.exists() and installed.exists() and login.exists(), "wizard install and login item")
        click("Open Carryover")
        wait(lambda: find("Devices", "push button"), "main window")
        print("PASS: invalid invite validation, wizard install, new group, autostart, main window", flush=True)
        # Regression: the first connected device used to populate an unbuilt
        # Linux native menu and panic, removing the tray icon entirely.
        peer_dir = root / "peer"
        code = json.loads(config.read_text())["group_key"]
        subprocess.run([binary, "--config-dir", str(peer_dir), "join"], input=code.encode(),
                       env=env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        peer = subprocess.Popen([binary, "--config-dir", str(peer_dir), "run", "--port", "0", "--name", "Linux Test Peer"],
                                env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait(lambda: find("Linux Test Peer", partial=True), "live device appears without crashing tray")
            time.sleep(1.2)
            assert tray_service(), "tray crashed after its connected-device list changed"
        finally:
            peer.terminate()
            peer.wait(timeout=5)
        print("PASS: live peer connection and device-list refresh preserve the tray process", flush=True)
        click("Add a Device")
        click("Show Link")
        wait(lambda: find("carryover://join?code=", partial=True), "invite link")
        click("Show QR Code")
        wait(lambda: find("Point it at this code.", partial=True), "QR instructions")
        click("Settings")
        click("Open at login", "check box", partial=True)
        wait(lambda: not login.exists(), "disable autostart")
        click("Open at login", "check box", partial=True)
        wait(login.exists, "enable autostart")
        click("Install updates automatically", "check box", partial=True)
        wait(lambda: not json.loads(config.read_text())["auto_update"], "persist update setting")
        click("Install updates automatically", "check box", partial=True)
        wait(lambda: json.loads(config.read_text())["auto_update"], "restore update setting")
        click("Open Log")
        wait(lambda: (root / "opened-log").exists(), "open log uses desktop opener")
        assert (root / "opened-log").read_text().strip() == str(config.parent / "copyblip.log")
        click("Check for Updates")
        wait(lambda: find("Couldn't check for updates.", partial=True), "reject unsigned update feed")
        print("PASS: invite link, QR page, login toggle, persisted update setting, log opener, unsigned update rejected", flush=True)

        def visible_panel():
            return subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Carryover Panel$"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0

        activate()
        wait(visible_panel, "popup from left-click activation")
        click("Pause Syncing", "check box", partial=True)
        wait(lambda: find("Paused", partial=True), "panel pauses sync")
        assert visible_panel(), "panel should stay open while changing a switch"
        click("Pause Syncing", "check box", partial=True)
        activate()
        wait(lambda: not visible_panel(), "second click hides popup")
        activate("ContextMenu")
        wait(visible_panel, "popup from right-click activation")
        wait(lambda: subprocess.check_output(["xdotool", "getactivewindow", "getwindowname"]).decode().strip() == "Carryover Panel", "popup gets keyboard focus")
        subprocess.run(["xdotool", "key", "Escape"], check=True)
        wait(lambda: not visible_panel(), "Escape dismisses popup")
        activate()
        wait(visible_panel, "popup reopens")
        main_id = subprocess.check_output(["xdotool", "search", "--name", "^Carryover$"]).splitlines()[0]
        subprocess.run(["xdotool", "windowactivate", main_id], check=True, timeout=3)
        wait(lambda: not visible_panel(), "focus loss dismisses popup")
        print("PASS: shared popup, left/right activation, pause, toggle, Escape, focus dismissal", flush=True)

        click("Recent")
        first = "Carryover UI history — first"
        second = "Carryover UI history — second"
        def copy(text):
            subprocess.run(["xclip", "-selection", "clipboard", "-target", "UTF8_STRING"],
                           input=text.encode(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        copy(first)
        wait(lambda: find(first, "push button", True), "first history card")
        copy(second)
        wait(lambda: find(second, "push button", True), "second history card")
        click(first, partial=True)
        wait(lambda: subprocess.check_output(["xclip", "-selection", "clipboard", "-out", "-target", "UTF8_STRING"]).decode() == first,
             "copy history item back")
        image = Path(__file__).resolve().parents[2] / "assets/icon.png"
        subprocess.run([binary, "clip", "--image", str(image)], env=env, check=True, stderr=subprocess.DEVNULL)
        wait(lambda: find("Image Sent", "push button", True), "history image card")
        copy("temporary text")
        wait(lambda: find("temporary text", "push button", True), "temporary history card")
        image_card = wait(lambda: find("Image Sent", "push button", True), "image card")
        assert image_card.queryAction().doAction(0)
        def image_copied():
            result = subprocess.run(["xclip", "-selection", "clipboard", "-out", "-target", "image/png"],
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            return result.returncode == 0 and result.stdout == image.read_bytes()
        wait(image_copied, "image copied back exactly")
        click("Clear All")
        click("Click again to clear")
        wait(lambda: json.loads((config.parent / "history/history.json").read_text()) == [], "clear history")
        print("PASS: text/image history, exact copy again, clear confirmation", flush=True)

        # Leave and rejoin through the wizard, keeping only the synthetic test key in memory.
        original_key = json.loads(config.read_text())["group_key"]
        click("Settings")
        click("Leave Group")
        click("Click again to leave")
        wait(lambda: find("Continue", "push button"), "setup after leaving group")
        assert not config.exists() and not login.exists()
        click("Continue")
        click("I already use Carryover", partial=True)
        entry = wait(lambda: find("carryover://join?code=…", "entry"), "join entry")
        bounds = entry.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        subprocess.run(["xdotool", "mousemove", str(bounds.x+10), str(bounds.y+10), "click", "1", "type", "--clearmodifiers",
                        "carryover://join?code=" + original_key], check=True)
        click("Set Up")
        wait(lambda: config.exists(), "join saved")
        assert json.loads(config.read_text())["group_key"] == original_key
        click("Open Carryover")
        wait(lambda: find("Devices", "push button"), "window after rejoin")
        print("PASS: leave group, erase local pairing, rejoin through setup", flush=True)
        activate()
        wait(lambda: find("Quit", "push button"), "popup quit command")
        click("Quit")
        wait(lambda: tray_service() is None, "quit stops desktop agent")
        subprocess.run(["gio", "launch", str(login)], env=env, check=True, stdout=subprocess.DEVNULL)
        wait(tray_service, "login entry restarts the chosen profile")
        launcher = root / "data/applications/com.coledeterman.carryover.desktop"
        subprocess.run(["gio", "launch", str(launcher)], env=env, check=True, stdout=subprocess.DEVNULL)
        wait(lambda: find("Devices", "push button"), "launcher opens running chosen profile")
        assert json.loads(config.read_text())["group_key"] == original_key
        print("PASS: popup Quit, real login-entry launch, launcher reopens custom profile", flush=True)
        activate()
        wait(lambda: find("Quit", "push button"), "quit before background test")
        click("Quit")
        wait(lambda: tray_service() is None, "agent exits before background test")
        subprocess.run([str(installed), "--config-dir", str(config.parent), "run", "--background", "--tray", "--port", "0", "--name", "Background Test"],
                       env=env, check=True, timeout=3)
        wait(tray_service, "background CLI starts tray process")
        subprocess.run(["gio", "launch", str(launcher)], env=env, check=True, stdout=subprocess.DEVNULL)
        wait(lambda: find("Devices", "push button"), "background process opens main window")
        print("PASS: Linux background CLI preserves profile and opens from launcher", flush=True)
        retained_config = config.read_bytes()
        subprocess.Popen([str(installed), "uninstall"], env=env, stdout=log, stderr=log)
        click("Yes")
        wait(lambda: not installed.exists(), "uninstall removes executable")
        assert config.read_bytes() == retained_config and not login.exists()
        assert not (root / "data/applications/com.coledeterman.carryover.desktop").exists()
        click("OK")
        print("PASS: graphical uninstall stops agent, removes launcher/login item, retains pairing", flush=True)
    finally:
        try:
            control("quit")
        except OSError:
            pass
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)
        # Also stop a setup or confirmation dialog orphaned by an assertion.
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                args = (entry / "cmdline").read_bytes().split(b"\0")
                if args and args[0] == os.fsencode(installed):
                    os.kill(int(entry.name), signal.SIGTERM)
            except (FileNotFoundError, ProcessLookupError, PermissionError):
                pass
        feed.shutdown()
        feed.server_close()
        log.flush()
        log.seek(0)
        print(log.read(), flush=True)
