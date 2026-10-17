"""Opt-in integration: run under dbus-run-session with built service binaries."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys

from gi.repository import Gio, GLib

REPO = Path(__file__).resolve().parents[2]
WORKSPACE_BUS = "org.rsynapse.Workspace"
WORKSPACE_ROOT = "/org/rsynapse/Workspace"
MANAGER = "org.rsynapse.Workspace.Manager1"
WINDOW = "org.rsynapse.Workspace.Window1"
WORKSPACE = "org.rsynapse.Workspace.Workspace1"
RELATION = "org.rsynapse.workspace.project"
ANCHOR = "org.rsynapse.window.workspace-project"


def exercise(args):
    directory = Path(args.artifacts)
    directory.mkdir(mode=0o700)
    runtime = directory / "runtime"
    runtime.mkdir(mode=0o700)
    config = directory / "config.toml"
    config.write_text('[general]\nautostart = []\nxwayland = false\n')
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_STATE_HOME=str(directory / "state"),
        LOCUS_RELATIONS_PATH=str(directory / "relations.json"), PROJD_STORE_PATH=str(directory / "projects.sqlite3"),
        WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="2", WLR_LIBINPUT_NO_DEVICES="1",
        XDG_CURRENT_DESKTOP="umbriel", GTK_A11Y="none", GTK_USE_PORTAL="0", GDK_DEBUG="no-portals",
        GIO_USE_VFS="local", CARBON_HOST="127.0.0.1", CARBON_PORT="9")
    for key in ("DISPLAY", "WAYLAND_DISPLAY", "UMBRIEL_SOCKET"):
        env.pop(key, None)
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    processes, logs, subscriptions = [], [], []
    waiting = {"predicate": None, "loop": None}
    ready = {"ipc": False}

    def call(service, path, interface, method, parameters=None):
        return bus.call_sync(service, path, interface, method, parameters, None,
            Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]

    def wake(*_):
        if waiting["predicate"] is not None:
            try:
                if waiting["predicate"]():
                    waiting["loop"].quit()
            except GLib.Error:
                pass

    def wait(predicate):
        def check():
            try:
                return predicate()
            except GLib.Error:
                return False
        if check():
            return
        waiting.update(predicate=predicate, loop=GLib.MainLoop())
        timer = GLib.timeout_add_seconds(15, lambda: (waiting["loop"].quit(), False)[1])
        waiting["loop"].run()
        waiting["predicate"] = None
        assert check(), "service/hotplug transition was not observed"
        GLib.source_remove(timer)

    def owner(name):
        return call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner",
            GLib.Variant("(s)", (name,)))

    def start(name, command):
        log = (directory / (name + ".log")).open("wb")
        logs.append(log)
        process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        processes.append(process)
        return process

    def request(command, **fields):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(env["UMBRIEL_SOCKET"])
            stream.sendall(json.dumps({"cmd": command, **fields}).encode() + b"\n")
            result = json.loads(stream.makefile().readline())
        assert "ok" in result, result
        return result["ok"]

    def properties(path, interface):
        return call(WORKSPACE_BUS, path, "org.freedesktop.DBus.Properties", "GetAll",
            GLib.Variant("(s)", (interface,)))

    def inventory():
        root = properties(WORKSPACE_ROOT, MANAGER)
        return {w["Id"]: w for path in root["Windows"] for w in [properties(path, WINDOW)]}

    def workspace(window):
        paths = inventory().get(window, {}).get("Workspace", [])
        return next((properties(path, WORKSPACE)["Id"] for path in paths), None)

    def records(relation):
        rows = call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List",
            GLib.Variant("(s)", (relation,)))
        return [dict(zip(("subject", "relation", "target", "metadata", "created", "updated"), row)) for row in rows]

    def assigned(identity, path):
        return any(record["subject"].get("id") == identity and record["target"].get("id") == str(path)
            for record in records(RELATION))

    def bind(identity, path):
        return call("org.rsynapse.Steward", "/org/rsynapse/Steward", "org.rsynapse.Steward.Associations1",
            "BindWorkspaceProject", GLib.Variant("(ss)", (identity, str(path))))

    try:
        subscriptions.append(bus.signal_subscribe(None, None, None, None, None, Gio.DBusSignalFlags.NONE, wake))
        compositor = subprocess.Popen([args.umbriel, "-c", str(config)], env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        processes.append(compositor)
        compositor_log = (directory / "compositor.log").open("wb")
        logs.append(compositor_log)
        def output(fd, condition):
            data = os.read(fd, 65536)
            compositor_log.write(data)
            compositor_log.flush()
            if b"IPC listening on" in data:
                ready["ipc"] = True
            wake()
            return bool(data)
        GLib.io_add_watch(compositor.stdout.fileno(), GLib.IO_IN | GLib.IO_HUP, output)
        wait(lambda: ready["ipc"])
        env.update(WAYLAND_DISPLAY="wayland-0", UMBRIEL_SOCKET=str(runtime / "umbriel-wayland-0.sock"))
        start("projection", [args.producer, "--backend", "umbriel"])
        start("locus", [args.locusd])
        start("projd", [args.projd])
        for name in (WORKSPACE_BUS, "org.rsynapse.Locus", "org.rsynapse.Proj"):
            wait(lambda: owner(name))
        wait(lambda: properties(WORKSPACE_ROOT, MANAGER)["Connected"])
        start("steward", [args.steward])
        wait(lambda: owner("org.rsynapse.Steward"))
        start("clients", [sys.executable, str(REPO / "workspace-dbus/service/tests/umbriel_live.py"), "--client"])
        wait(lambda: len(inventory()) == 2)
        native = request("windows")
        first = next(w["id"] for w in native if w["title"] == "Umbriel probe A")
        second = next(w["id"] for w in native if w["title"] == "Umbriel probe B")
        project_a, project_b, project_c = [directory / name for name in ("project-a", "project-b", "project-c")]
        for path in (project_a, project_b, project_c):
            path.mkdir()
        bind(workspace(first), project_a)
        request("output-create", arg="project-hotplug 800x600@30")
        request("msg", arg=f"window-focus:{second}")
        request("msg", arg="window-move-to-workspace:1/project-hotplug")
        wait(lambda: workspace(second) is not None and workspace(second).startswith("project-hotplug:"))
        original = workspace(second)
        bind(original, project_b)
        wait(lambda: any(record["subject"].get("id") == second and record["target"].get("id") == str(project_b)
            for record in records(ANCHOR)))
        # Keep an unassigned active destination on either remaining output.
        # If evacuation lands in an explicitly bound workspace, that existing
        # user's choice correctly wins instead of recovering the old project.
        for output in ("HEADLESS-1", "HEADLESS-2"):
            blank = max((w for w in request("workspaces") if w["output"] == output), key=lambda w: w["index"])
            assert blank["window_count"] == 0
            request("msg", arg=f'workspace-switch:{blank["index"]}/{output}')
        request("msg", arg="workspace-switch:1/project-hotplug")
        request("output-destroy", arg="project-hotplug")
        wait(lambda: workspace(second) is not None and workspace(second) != original)
        replacement = workspace(second)
        wait(lambda: assigned(replacement, project_b))
        assert not assigned(original, project_b)
        assert len(inventory()) == 2
        assert assigned(workspace(first), project_a)
        # A normal explicit selection must still override recovered context.
        chosen = bind(replacement, project_c)
        wait(lambda: assigned(replacement, project_c))
        # A replacement monitor can reuse its connector and native counters.
        # The public API must expose a fresh workspace identity for that life.
        request("output-create", arg="project-hotplug 800x600@30")
        request("msg", arg=f"window-focus:{second}")
        request("msg", arg="window-move-to-workspace:1/project-hotplug")
        wait(lambda: workspace(second) is not None and workspace(second).startswith("project-hotplug:"))
        reincarnated = workspace(second)
        assert reincarnated != original, "native workspace ID reuse leaked into the public API"
        wait(lambda: assigned(reincarnated, project_c))
        for output in ("HEADLESS-1", "HEADLESS-2"):
            blank = max((w for w in request("workspaces") if w["output"] == output), key=lambda w: w["index"])
            assert blank["window_count"] == 0
            request("msg", arg=f'workspace-switch:{blank["index"]}/{output}')
        request("msg", arg="workspace-switch:1/project-hotplug")
        request("output-destroy", arg="project-hotplug")
        wait(lambda: workspace(second) is not None and workspace(second) != reincarnated)
        replacement = workspace(second)
        wait(lambda: assigned(replacement, project_c))
        call("org.rsynapse.Proj", "/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "RemoveProject",
            GLib.Variant("(s)", (chosen,)))
        wait(lambda: not assigned(replacement, project_c))
        wait(lambda: not any(record["metadata"].get("project-id") == chosen for record in records(ANCHOR)))
        print(json.dumps({"real_umbriel_hotplug": True, "replacement_workspace": True,
            "project_binding_recovered": True, "explicit_reassignment": True,
            "project_removal_cleans_anchors": True, "windows_preserved": True,
            "reused_connector_has_fresh_workspace_identity": True}))
    except Exception:
        diagnostic = {}
        for name, read in (("bindings", lambda: records(RELATION)), ("anchors", lambda: records(ANCHOR)),
                           ("windows", inventory), ("native_workspaces", lambda: request("workspaces"))):
            try:
                diagnostic[name] = read()
            except Exception as error:
                diagnostic[name] = str(error)
        (directory / "failure.json").write_text(json.dumps(diagnostic, indent=2))
        raise
    finally:
        for token in subscriptions:
            bus.signal_unsubscribe(token)
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for log in logs:
            log.close()


if __name__ == "__main__":
    if "RSYNAPSE_HOTPLUG_TEST_WORKER" not in os.environ:
        result = subprocess.run(["dbus-run-session", "--", sys.executable, __file__, *sys.argv[1:]],
            env=dict(os.environ, RSYNAPSE_HOTPLUG_TEST_WORKER="1"))
        sys.exit(result.returncode)
    parser = argparse.ArgumentParser()
    parser.add_argument("--steward", required=True)
    parser.add_argument("--producer", default=str(REPO / "workspace-dbus/target/release/workspace-dbus"))
    parser.add_argument("--locusd", default=str(Path.home() / ".local/bin/locusd"))
    parser.add_argument("--projd", default=str(Path.home() / ".local/bin/projd"))
    parser.add_argument("--umbriel", default="umbriel")
    parser.add_argument("--artifacts", required=True)
    exercise(parser.parse_args())
