"""Run the real layer-shell component on an isolated headless Wayland display."""
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile
import time


def main():
    root = Path(sys.argv[1])
    compositor_bin = os.environ.get("UMBRIEL_TEST_BIN") or shutil.which("umbriel")
    if not compositor_bin:
        raise SystemExit("Set UMBRIEL_TEST_BIN to a headless Umbriel binary, or explicitly opt into RSYNAPSE_UI_LIVE_DISPLAY=1")
    with tempfile.TemporaryDirectory(prefix="rsynapse-overlay-", ignore_cleanup_errors=True) as directory:
        directory = Path(directory)
        runtime = directory / "runtime"
        runtime.mkdir(mode=0o700)
        config = directory / "config.toml"
        config.write_text("""[general]
autostart = []
xwayland = false
[animation]
enabled = false
[output.HEADLESS-1]
workspaces = 1
""")
        env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_STATE_HOME=str(directory / "state"),
                   WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_LIBINPUT_NO_DEVICES="1",
                   WLR_RENDERER="pixman", GDK_BACKEND="wayland", GDK_DEBUG="no-portals")
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "UMBRIEL_SOCKET"):
            env.pop(name, None)
        compositor = subprocess.Popen([compositor_bin, "-c", str(config)], env=env,
                                      stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        startup = b""
        try:
            with selectors.DefaultSelector() as events:
                events.register(compositor.stdout, selectors.EVENT_READ)
                deadline = time.monotonic() + 20
                while b"IPC listening on" not in startup:
                    if not events.select(max(0, deadline - time.monotonic())):
                        raise RuntimeError("headless compositor startup timed out")
                    chunk = os.read(compositor.stdout.fileno(), 65536)
                    if not chunk:
                        raise RuntimeError("headless compositor exited: " + startup.decode(errors="replace"))
                    startup += chunk
            env.update(WAYLAND_DISPLAY="wayland-0", UMBRIEL_SOCKET=str(runtime / "umbriel-wayland-0.sock"))
            subprocess.run(["dbus-update-activation-environment", "XDG_RUNTIME_DIR", "XDG_STATE_HOME",
                            "WAYLAND_DISPLAY", "GDK_BACKEND", "GDK_DEBUG"], env=env, check=True)
            command = ["cargo", "test", "--manifest-path", str(root / "shell/Cargo.toml"),
                       "-p", "rsynapse-workspace", "workspace_ui_roundtrip", "--", "--ignored", "--nocapture", "--test-threads=1"]
            return subprocess.run(command, env=env).returncode
        finally:
            compositor.terminate()
            try:
                compositor.wait(timeout=5)
            except subprocess.TimeoutExpired:
                compositor.kill()
                compositor.wait()
            # A private desktop portal may have mounted its own document FUSE
            # filesystem here. Detach only that test-owned mount before deleting
            # the runtime; the live session's runtime is never touched.
            unmount = shutil.which("fusermount3") or shutil.which("fusermount")
            if unmount:
                subprocess.run([unmount, "-u", "-z", str(runtime / "doc")],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == "__main__":
    sys.exit(main())
