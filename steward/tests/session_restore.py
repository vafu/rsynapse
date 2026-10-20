"""Two real compositor sessions on separate private buses and isolated state.

No activation service directories, host systemd, installed changes or live
application instances are used. Requires Umbriel and Python GI/GTK4.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import socket

from gi.repository import Gio, GLib

ROOT = Path(__file__).resolve().parents[2]
BUS = "org.rsynapse.Workspace"
PATH = "/org/rsynapse/Workspace"
MANAGER = BUS + ".Manager1"
WINDOW = BUS + ".Window1"
WORKSPACE = BUS + ".Workspace1"
APP = "org.rsynapse.SessionProbe"


def probe(args):
    import gi
    gi.require_version("Gtk", "4.0")
    from gi.repository import Gtk
    app = Gtk.Application(application_id=args.app_id, flags=Gio.ApplicationFlags.NON_UNIQUE)
    def activate(app):
        if args.report_dir:
            Path(args.report_dir, args.app_id + ".json").write_text(json.dumps({"cwd": os.getcwd()}))
        window = Gtk.ApplicationWindow(application=app, title="Session restore probe")
        window.set_default_size(300, 200)
        window.present()
    app.connect("activate", activate)
    return app.run([])


def worker(args):
    root = Path(args.artifacts)
    directory = root / args.phase
    directory.mkdir(mode=0o700)
    runtime = directory / "runtime"
    runtime.mkdir(mode=0o700)
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("UWSM_") or key in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "UMBRIEL_SOCKET",
                "NIRI_SOCKET", "NOTIFY_SOCKET", "MANAGERPID", "INVOCATION_ID", "SYSTEMD_EXEC_PID",
                "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"):
            env.pop(key, None)
    env.update(HOME=str(root / "home"), XDG_RUNTIME_DIR=str(runtime),
        XDG_CONFIG_HOME=str(root / "config"), XDG_STATE_HOME=str(root / "state"),
        XDG_DATA_HOME=str(root / "data"), XDG_CACHE_HOME=str(root / "cache"),
        XDG_CURRENT_DESKTOP="umbriel", XDG_SESSION_DESKTOP="umbriel",
        WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="2" if args.phase in ("capture", "hotplug") else "1", WLR_LIBINPUT_NO_DEVICES="1",
        GTK_A11Y="none", NO_AT_BRIDGE="1", GTK_USE_PORTAL="0", GDK_BACKEND="wayland",
        GDK_DEBUG="no-portals", GIO_USE_VFS="local", CARBON_PORT="9",
        LOCUS_RELATIONS_PATH=str(root / "relations.json"), PROJD_STORE_PATH=str(root / "projects.sqlite3"),
        STEWARD_SESSION_STORE_PATH=str(root / "sessions.json"),
        STEWARD_SESSION_LAUNCHERS_PATH=str(root / "launchers.json"))
    config = directory / "umbriel.toml"
    session_rules = ROOT / "steward/umbriel-session-rules.toml"
    config.write_text("[include]\nfiles = [" + json.dumps(str(session_rules)) + "]\n"
        "[general]\nautostart = []\nxwayland = false\n")
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    waiting = {"predicate": None, "loop": None, "checking": False}
    ready = {"ipc": False}
    processes, logs, subscriptions = [], [], []

    def call(service, path, interface, method, parameters=None):
        result = bus.call_sync(service, path, interface, method, parameters, None,
            Gio.DBusCallFlags.NO_AUTO_START, 15000, None).unpack()
        return result[0] if result else None

    def wake(*_):
        if waiting["predicate"] and not waiting["checking"]:
            waiting["checking"] = True
            try:
                if waiting["predicate"]():
                    waiting["loop"].quit()
            except GLib.Error:
                pass
            finally:
                waiting["checking"] = False

    def wait(predicate):
        def check():
            try:
                return predicate()
            except GLib.Error:
                return False
        if check():
            return
        waiting.update(predicate=predicate, loop=GLib.MainLoop())
        timer = GLib.timeout_add_seconds(25, lambda: (waiting["loop"].quit(), False)[1], priority=GLib.PRIORITY_HIGH)
        waiting["loop"].run()
        waiting["predicate"] = None
        assert check(), "isolated transition not observed"
        GLib.source_remove(timer)

    def start(name, command, **kwargs):
        log = (directory / (name + ".log")).open("wb")
        logs.append(log)
        process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, **kwargs)
        processes.append(process)
        return process

    def properties(path, interface):
        return call(BUS, path, "org.freedesktop.DBus.Properties", "GetAll", GLib.Variant("(s)", (interface,)))

    def windows():
        return {w["Id"]: w for path in properties(PATH, MANAGER)["Windows"] for w in [properties(path, WINDOW)]}

    def workspace(window):
        return properties(windows()[window]["Workspace"][0], WORKSPACE)["Id"]

    def control(method, signature, *arguments):
        return call(BUS, PATH, BUS + ".Control1", method,
            GLib.Variant(signature, (properties(PATH, MANAGER)["Generation"], *arguments)))

    def bind(window, project):
        return call("org.rsynapse.Steward", "/org/rsynapse/Steward", "org.rsynapse.Steward.Associations1",
            "BindWorkspaceProject", GLib.Variant("(ss)", (workspace(window), str(root / project))))

    def cli(*arguments, success=True):
        process = Gio.Subprocess.new([args.steward, "session", *arguments],
            Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE)
        # Gio.Subprocess inherits os.environ, set to the isolated child env below.
        result = {}
        loop = GLib.MainLoop()
        def complete(process, task):
            try:
                result["output"] = process.communicate_utf8_finish(task)
            except GLib.Error as error:
                result["error"] = str(error)
            loop.quit()
        process.communicate_utf8_async(None, None, complete)
        timer = GLib.timeout_add_seconds(60, lambda: (process.force_exit(), loop.quit(), False)[2])
        loop.run()
        GLib.source_remove(timer)
        assert "output" in result, result
        _, output, error = result["output"]
        assert process.get_successful() == success, (output, error)
        return json.loads(output)

    try:
        os.environ.clear()
        os.environ.update(env)
        subscriptions.append(bus.signal_subscribe(None, None, None, None, None, Gio.DBusSignalFlags.NONE, wake))
        compositor_log = (directory / "compositor.log").open("wb")
        logs.append(compositor_log)
        compositor = subprocess.Popen([args.umbriel, "-c", str(config)], env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        processes.append(compositor)
        pending = bytearray()
        def output(fd, condition):
            data = os.read(fd, 65536)
            compositor_log.write(data)
            compositor_log.flush()
            pending.extend(data)
            while b"\n" in pending:
                line, _, rest = pending.partition(b"\n")
                pending[:] = rest
                if b"IPC listening on" in line:
                    ready["ipc"] = True
            wake()
            return bool(data)
        GLib.io_add_watch(compositor.stdout.fileno(), GLib.IO_IN | GLib.IO_HUP, output)
        wait(lambda: ready["ipc"])
        env.update(WAYLAND_DISPLAY="wayland-0", UMBRIEL_SOCKET=str(runtime / "umbriel-wayland-0.sock"))
        os.environ.update(env)
        start("projection", [args.producer, "--backend", "umbriel"])
        start("locus", [args.locusd])
        start("projd", [args.projd])
        for service in (BUS, "org.rsynapse.Locus", "org.rsynapse.Proj"):
            wait(lambda: call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                "NameHasOwner", GLib.Variant("(s)", (service,))))
        wait(lambda: properties(PATH, MANAGER)["Connected"])
        steward = start("steward", [args.steward, "associations"])
        wait(lambda: call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "NameHasOwner", GLib.Variant("(s)", ("org.rsynapse.Steward",))))
        if args.phase == "hotplug":
            from session_hotplug import exercise
            def restart_steward():
                steward.terminate()
                steward.wait(timeout=5)
                wait(lambda: not call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                    "NameHasOwner", GLib.Variant("(s)", ("org.rsynapse.Steward",))))
                start("steward-restarted", [args.steward, "associations"])
                wait(lambda: call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                    "NameHasOwner", GLib.Variant("(s)", ("org.rsynapse.Steward",))))
            exercise(root, start, wait, properties, windows, workspace, control, bind, cli, restart_steward)
        elif args.phase == "silent":
            from session_silent import exercise
            exercise(root, start, wait, properties, windows, workspace, control, cli)
        elif args.phase == "capture":
            try:
                call(BUS, PATH, BUS + ".Control1", "SelectWorkspace",
                    GLib.Variant("(tsu)", (properties(PATH, MANAGER)["Generation"] + 1, "HEADLESS-1", 1)))
                raise AssertionError("stale generation control was accepted")
            except GLib.Error:
                pass
            window_ids = []
            for index, project in enumerate(("project-a", "project-b"), 1):
                selected = control("SelectWorkspace", "(tsu)", "HEADLESS-" + str(index), 1)
                before = set(windows())
                start("probe-" + project, [sys.executable, __file__, "--probe", "--app-id", APP])
                wait(lambda: len(windows()) > len(before))
                window = next(iter(set(windows()) - before))
                control("PlaceWindow", "(tssb)", window, selected, False)
                bind(window, project)
                if index == 2:
                    control("SetLayout", "(tss)", workspace(window), "master")
                    control("PlaceWindow", "(tssb)", window, workspace(window), True)
                window_ids.append(window)
            saved = cli("save", "work")
            assert len(saved["workspaces"]) == 2, saved
            assert sum(len(w["windows"]) for w in saved["workspaces"]) == 2
            assert not any(identity in json.dumps(saved) for identity in window_ids)
            (root / "capture.json").write_text(json.dumps({"saved": saved, "windows": window_ids}))
            def saved_session(name):
                return bus.call_sync("org.rsynapse.Steward", "/org/rsynapse/Steward", "org.rsynapse.Steward.Session1",
                    "Get", GLib.Variant("(s)", (name,)), None, Gio.DBusCallFlags.NO_AUTO_START, 15000, None).unpack()
            reference = saved_session("work")
            wait(lambda: saved_session("last")[2] == reference[2])
            # Terminate the isolated compositor first: source loss must freeze
            # capture instead of replacing the last desktop with empty state.
            compositor.terminate()
            wait(lambda: not properties(PATH, MANAGER)["Connected"])
            assert saved_session("last")[2] == reference[2]
            durable = call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List",
                GLib.Variant("(s)", ("org.rsynapse.session.workspace.project",)))
            assert len(durable) == 2
        else:
            before = cli("show", "work")
            assert not windows()
            first = cli("restore", "work")
            assert first["restored"] == 2 and not first["issues"], first
            second = cli("restore", "work")
            assert second["restored"] == 0 and second["reused"] == 2, second
            assert len(windows()) == 2
            saved = json.loads((root / "capture.json").read_text())
            assert set(windows()).isdisjoint(saved["windows"])
            assert before == saved["saved"]
            reports = [json.loads(path.read_text())["cwd"] for path in (root / "reports").glob("*.json")]
            assert sorted(reports) == sorted(str(root / p) for p in ("project-a", "project-b")), reports
            records = call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List",
                GLib.Variant("(s)", ("org.rsynapse.workspace.project",)))
            bindings = {r[0]["id"]: r[2]["id"] for r in records}
            assert sorted(bindings.values()) == sorted(str(root / p) for p in ("project-a", "project-b"))
            assert all(workspace(window) in bindings for window in windows())
            assert sum(w["Floating"] for w in windows().values()) == 1
            assert all(properties(w["Workspace"][0], WORKSPACE)["Output"] == properties(PATH, MANAGER)["Outputs"] for w in windows().values())
            b = next(window for window in windows() if bindings[workspace(window)] == str(root / "project-b"))
            call("org.rsynapse.Steward", "/org/rsynapse/Steward", "org.rsynapse.Steward.Associations1",
                "UnassignWorkspaceProject", GLib.Variant("(s)", (workspace(b),)))
            cli("save", "unassigned")
            again = cli("restore", "work")
            assert again["reused"] == 2 and not again["issues"], again
            projects = call("org.rsynapse.Proj", "/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "ListProjects")
            removed = next(p[0] for p in projects if p[2] == str(root / "project-b"))
            call("org.rsynapse.Proj", "/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "RemoveProject",
                GLib.Variant("(s)", (removed,)))
            partial = cli("restore", "work", success=False)
            assert any("removed" in issue for issue in partial["issues"]), partial
            projects = call("org.rsynapse.Proj", "/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "ListProjects")
            assert all(p[0] != removed and p[2] != str(root / "project-b") for p in projects)
            def no_deleted_project_link():
                rows = call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List",
                    GLib.Variant("(s)", ("org.rsynapse.session.workspace.project",)))
                return all(row[2].get("id") != removed for row in rows)
            wait(no_deleted_project_link)
            assert len(windows()) == 2
            print(json.dumps({"two_private_sessions": True, "bindings_restored": True,
                "windows_relaunched": True, "project_cwds": True, "idempotent": True, "floating": True,
                "missing_output_fallback": True, "immutable_checkpoint_bindings": True,
                "removed_project_not_recreated": True, "stale_epoch_rejected": True}))
    except Exception:
        diagnostic = {}
        for name, reader in (("manager", lambda: properties(PATH, MANAGER)), ("windows", windows),
            ("relations", lambda: call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List", GLib.Variant("(s)", ("",))))):
            try:
                diagnostic[name] = reader()
            except Exception as error:
                diagnostic[name] = str(error)
        for name in ("workspaces", "windows", "outputs"):
            try:
                with socket.socket(socket.AF_UNIX) as stream:
                    stream.settimeout(3)
                    stream.connect(env["UMBRIEL_SOCKET"])
                    stream.sendall(json.dumps({"cmd":name}).encode()+b"\n")
                    diagnostic["native_"+name] = json.loads(stream.makefile().readline())
            except Exception as error:
                diagnostic["native_"+name] = str(error)
        (directory / "failure.json").write_text(json.dumps(diagnostic, indent=2))
        raise
    finally:
        for subscription in subscriptions:
            bus.signal_unsubscribe(subscription)
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts")
    parser.add_argument("--steward", default=str(ROOT / "steward/target/debug/rsynapse-steward"))
    parser.add_argument("--producer", default=str(ROOT / "workspace-dbus/target/debug/rsynapse-workspace-dbus"))
    parser.add_argument("--locusd", default=str(Path.home() / ".local/bin/rsynapse-locusd"))
    parser.add_argument("--projd", default=str(Path.home() / ".local/bin/rsynapse-projd"))
    parser.add_argument("--umbriel", default="umbriel")
    parser.add_argument("--phase", choices=("capture", "restore", "hotplug", "silent"))
    parser.add_argument("--hotplug", action="store_true", help="test same-session output removal/group preservation")
    parser.add_argument("--silent", action="store_true", help="test background launch rules, silent moves and final focus")
    parser.add_argument("--probe", action="store_true")
    parser.add_argument("--app-id", default=APP)
    parser.add_argument("--report-dir")
    args = parser.parse_args()
    if args.probe:
        return probe(args)
    if args.phase:
        return worker(args)
    root = Path(args.artifacts).resolve()
    root.mkdir(mode=0o700)
    for name in ("home", "config", "state", "data", "cache", "reports", "project-a", "project-b", "project-c"):
        (root / name).mkdir(mode=0o700)
    app = APP + ".rsynapse.r{restore_id}"
    (root / "launchers.json").write_text(json.dumps({APP: {
        "argv": [sys.executable, str(Path(__file__).resolve()), "--probe", "--app-id", app,
            "--report-dir", str(root / "reports")], "match_app_id": app}}))
    config = root / "bus.conf"
    config.write_text('<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" '
        '"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">\n'
        f'<busconfig><type>session</type><listen>unix:tmpdir={root}</listen><auth>EXTERNAL</auth>'
        '<policy context="default"><allow send_destination="*" eavesdrop="true"/>'
        '<allow eavesdrop="true"/><allow own="*"/></policy></busconfig>')
    arguments = sys.argv[1:]
    for phase in (("hotplug",) if args.hotplug else ("capture", "silent") if args.silent else ("capture", "restore")):
        subprocess.run(["dbus-run-session", "--config-file=" + str(config), "--", sys.executable,
            str(Path(__file__).resolve()), *arguments, "--phase", phase], check=True)


if __name__ == "__main__":
    sys.exit(main())
