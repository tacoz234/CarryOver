#!/usr/bin/env python3
"""Two real agents, two independent X11 clipboards, real mDNS and encrypted sync."""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/copyblip").resolve())
processes = []

def wait(check, message):
    for _ in range(150):
        try:
            if check():
                return
        except subprocess.CalledProcessError:
            pass
        time.sleep(0.1)
    raise AssertionError(message)

with tempfile.TemporaryDirectory(prefix="carryover-real-sync-") as temporary:
    root = Path(temporary)
    environments = []
    try:
        for index in range(2):
            read_fd, write_fd = os.pipe()
            processes.append(subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1024x768x24"],
                             pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
            os.close(write_fd)
            with os.fdopen(read_fd) as pipe:
                display = pipe.readline().strip()
            assert display
            env = dict(os.environ, DISPLAY=":" + display)
            env.pop("WAYLAND_DISPLAY", None)
            environments.append(env)
        def command(index, *args, **kwargs):
            return subprocess.run([binary, "--config-dir", str(root / str(index)), *args],
                                  env=environments[index], stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, check=True, **kwargs).stdout
        command(0, "init")
        code = command(0, "pairing-code")
        command(1, "join", input=code)
        for i in range(2):
            command(i, "clip", f"local clipboard {i}")
            with (root / f"{i}.log").open("w") as log:
                processes.append(subprocess.Popen([binary, "--config-dir", str(root / str(i)), "run", "--port", "0", "--name", f"Linux {i}"],
                                 env=environments[i], stdout=log, stderr=log))
        wait(lambda: all("connected to Linux" in (root / f"{i}.log").read_text() for i in range(2)), "mDNS did not connect the two agents")
        assert command(0, "clip").strip() == b"local clipboard 0"
        assert command(1, "clip").strip() == b"local clipboard 1"
        image = Path(__file__).resolve().parents[2] / "assets/icon.png"
        for sender in range(2):
            receiver = 1 - sender
            text = f"real Linux clipboard {sender} — λ\n"
            command(sender, "clip", text)
            wait(lambda: command(receiver, "clip") == (text + "\n").encode(), "text did not sync exactly")
            command(sender, "clip", "--image", str(image))
            destination = root / f"received-{receiver}.png"
            def received_image():
                command(receiver, "clip", "--save-image", str(destination))
                return destination.read_bytes() == image.read_bytes()
            wait(received_image, "PNG did not sync exactly")
        time.sleep(0.7)
        for i in range(2):
            log = (root / f"{i}.log").read_text()
            assert log.count("sending ") == 2, "a received clipboard was echoed back"
        print("PASS: two real agents discovered via mDNS; startup preserves both clipboards; Unicode/newlines and PNG sync both ways without echoes.")
    finally:
        for process in reversed(processes):
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
