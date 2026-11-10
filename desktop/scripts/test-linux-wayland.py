#!/usr/bin/env python3
"""Clipboard smoke checks on an isolated, headless Sway compositor. Run as non-root."""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

binary = str(Path(sys.argv[1] if len(sys.argv)>1 else "target/debug/copyblip").resolve())
with tempfile.TemporaryDirectory(prefix="carryover-wayland-") as temporary:
    root = Path(temporary)
    runtime = root / "runtime"
    runtime.mkdir(mode=0o700)
    config = root / "sway.conf"
    config.write_text("output * mode 1280x720\n")
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
               WLR_LIBINPUT_NO_DEVICES="1", WLR_RENDERER="pixman")
    env.pop("DISPLAY", None)
    with (root / "sway.log").open("w") as log:
        compositor = subprocess.Popen(["sway", "--unsupported-gpu", "-c", str(config)], env=env, stdout=log, stderr=log)
    try:
        for _ in range(100):
            sockets = [p for p in runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                break
            if compositor.poll() is not None:
                raise AssertionError((root / "sway.log").read_text())
            time.sleep(0.1)
        else:
            raise AssertionError("Wayland socket did not appear")
        env["WAYLAND_DISPLAY"] = sockets[0].name
        def clip(*args):
            return subprocess.check_output([binary, "clip", *args], env=env)
        text = "Wayland clipboard — λ\n"
        subprocess.run(["wl-copy", "--type", "text/plain;charset=utf-8"], input=text.encode(), env=env, check=True)
        assert clip() == (text + "\n").encode()
        clip("new Wayland text")
        assert subprocess.check_output(["wl-paste", "--no-newline"], env=env) == b"new Wayland text"
        image = Path(__file__).resolve().parents[2] / "assets/icon.png"
        clip("--image", str(image))
        assert subprocess.check_output(["wl-paste", "--no-newline", "--type", "image/png"], env=env) == image.read_bytes()
        destination = root / "copied.png"
        clip("--save-image", str(destination))
        assert destination.read_bytes() == image.read_bytes()
        print("PASS: native Wayland Unicode, exact newlines, external owners, PNG write/read.")
    finally:
        compositor.terminate()
        try:
            compositor.wait(timeout=3)
        except subprocess.TimeoutExpired:
            compositor.kill()
            compositor.wait()
