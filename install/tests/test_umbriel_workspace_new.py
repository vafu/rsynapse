"""Real, isolated compositor check: dbus-run-session -- python3 this-file.py."""

import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

HELPER = Path(__file__).resolve().parents[1] / "bin" / "umbriel-workspace-new"


def client():
    import gi
    gi.require_version("Gtk", "4.0")
    from gi.repository import Gio, Gtk
    app = Gtk.Application(application_id="org.rsynapse.WorkspaceInsertionProbe",
        flags=Gio.ApplicationFlags.NON_UNIQUE)
    def activate(app):
        for title in ("Insertion A", "Insertion B"):
            window = Gtk.ApplicationWindow(application=app, title=title)
            window.set_default_size(320, 240)
            window.present()
    app.connect("activate", activate)
    return app.run([])


def exercise():
    with tempfile.TemporaryDirectory(prefix="umbriel-insertion-") as temporary:
        directory = Path(temporary)
        runtime = directory / "runtime"
        runtime.mkdir(mode=0o700)
        config = directory / "config.toml"
        config.write_text('[general]\nautostart = []\nxwayland = false\n'
            '[output.HEADLESS-1]\nfocus_at_startup = true\n')
        environment = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), WLR_BACKENDS="headless",
            WLR_HEADLESS_OUTPUTS="2", WLR_LIBINPUT_NO_DEVICES="1", GTK_A11Y="none",
            GTK_USE_PORTAL="0", GDK_BACKEND="wayland", GDK_DEBUG="no-portals", GIO_USE_VFS="local")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "UMBRIEL_SOCKET"):
            environment.pop(key, None)
        compositor = subprocess.Popen(["umbriel", "-c", str(config)], env=environment,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        probe = None
        try:
            for line in iter(compositor.stdout.readline, b""):
                if b"IPC listening on" in line:
                    break
            else:
                raise AssertionError("headless Umbriel did not start")
            native_socket = str(runtime / "umbriel-wayland-0.sock")
            environment.update(UMBRIEL_SOCKET=native_socket, WAYLAND_DISPLAY="wayland-0")

            def request(command, **fields):
                with socket.socket(socket.AF_UNIX) as connection:
                    connection.settimeout(10)
                    connection.connect(native_socket)
                    connection.sendall(json.dumps({"cmd": command, **fields}).encode() + b"\n")
                    with connection.makefile("rb") as response:
                        result = json.loads(response.readline())
                assert "err" not in result, result
                return result["ok"]

            def action(value):
                return request("msg", arg=value)

            def select(identity):
                workspace = next(w for w in request("workspaces") if w["id"] == identity)
                action(f'workspace-switch:{workspace["index"]}/{workspace["output"]}')

            # Wait for actual client map events, without readiness sleeps/polling.
            with socket.socket(socket.AF_UNIX) as subscription:
                subscription.settimeout(15)
                subscription.connect(native_socket)
                subscription.sendall(b'{"cmd":"subscribe","events":["windows"]}\n')
                probe = subprocess.Popen([sys.executable, __file__, "--client"], env=environment,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                with subscription.makefile("rb") as events:
                    for line in events:
                        frame = json.loads(line)
                        if frame["event"] == "windows" and len(frame["data"]) == 2:
                            windows = frame["data"]
                            break
                    else:
                        raise AssertionError("probe windows did not map")
            first = next(w for w in windows if w["title"] == "Insertion A")
            second = next(w for w in windows if w["title"] == "Insertion B")
            output = first["output"]
            action("window-focus:" + second["id"])
            action(f"window-move-to-workspace:2/{output}")
            subprocess.run(["umbriel", "settle"], env=environment, check=True)
            anchors = {w["id"]: w["workspace"] for w in request("windows")}
            other_ids = {w["id"] for w in request("workspaces") if w["output"] != output}
            checks = []

            for anchor, direction in [(first["id"], "above"), (first["id"], "below"),
                                      (second["id"], "above"), (second["id"], "below")]:
                select(anchors[anchor])
                insertion = subprocess.run([sys.executable, str(HELPER), direction], env=environment,
                    capture_output=True, text=True)
                assert insertion.returncode == 0, (direction, insertion.stderr, request("workspaces"))
                inserted = insertion.stdout.strip()
                snapshot = request("workspaces")
                blank = next(w for w in snapshot if w["id"] == inserted)
                original = next(w for w in snapshot if w["id"] == anchors[anchor])
                assert blank["focused"] and blank["active"] and blank["window_count"] == 0, blank
                assert blank["index"] == original["index"] + (-1 if direction == "above" else 1), snapshot
                assert {w["id"]: w["workspace"] for w in request("windows")} == anchors
                assert {w["id"] for w in snapshot if w["output"] != output} == other_ids
                # Repeated insertion from an already blank workspace reuses it.
                again = subprocess.run([sys.executable, str(HELPER), "above"], env=environment,
                    check=True, capture_output=True, text=True).stdout.strip()
                assert again == inserted
                # Native keyboard return leaves the empty workspace and allows
                # normal pruning, while preserving required edge sentinels.
                interior = blank["index"] < max(w["index"] for w in snapshot if w["output"] == output)
                action("workspace-focus-last")
                subprocess.run(["umbriel", "settle"], env=environment, check=True)
                closed = request("workspaces")
                assert next(w for w in closed if w["focused"])["id"] == original["id"], closed
                if interior:
                    assert inserted not in {w["id"] for w in closed}, closed
                assert {w["id"]: w["workspace"] for w in request("windows")} == anchors
                checks.append(direction)

            # The optional leading sentinel also stays ahead of an insertion.
            config.write_text(config.read_text() + "\n[workspaces]\nempty_above = true\n")
            action("config-reload")
            select(anchors[first["id"]])
            inserted = subprocess.run([sys.executable, str(HELPER), "above"], env=environment,
                check=True, capture_output=True, text=True).stdout.strip()
            snapshot = request("workspaces")
            assert next(w for w in snapshot if w["id"] == inserted)["index"] == 2
            print(json.dumps({"real_umbriel": True, "insertions": checks, "leading_empty": True,
                "window_identities_preserved": True, "other_output_unchanged": True, "empty_reuse": True,
                "native_close_return": True, "unused_interior_pruned": True}))
        finally:
            for process in (probe, compositor):
                if process is not None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == "__main__":
    if "--client" in sys.argv:
        sys.exit(client())
    exercise()
