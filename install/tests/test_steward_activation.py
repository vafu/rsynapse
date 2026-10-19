"""Smoke-test rendered Steward D-Bus activation on a private bus, without systemd.

Uses installed binaries by default, isolated HOME/XDG/service stores, and a
disconnected projection. No compositor or working-session services are touched.
"""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import signal
import shlex

from gi.repository import Gio, GLib

REPO = Path(__file__).resolve().parents[2]


def isolated_environment(root):
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("UWSM_") or key in ("NOTIFY_SOCKET", "MANAGERPID", "INVOCATION_ID",
                "SYSTEMD_EXEC_PID", "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "UMBRIEL_SOCKET", "NIRI_SOCKET",
                "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE"):
            env.pop(key, None)
    for name in ("HOME", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"):
        path = root / name.lower()
        path.mkdir(mode=0o700, exist_ok=True)
        env[name] = str(path)
    env.update(DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent-system-bus"), CARBON_PORT="9",
        LOCUS_RELATIONS_PATH=str(root / "relations.json"), PROJD_STORE_PATH=str(root / "projects.sqlite3"),
        PROJD_GOALS_DIR=str(root / "goals"),
        STEWARD_SESSION_STORE_PATH=str(root / "sessions.json"),
        STEWARD_SESSION_LAUNCHERS_PATH=str(root / "launchers.json"),
        XDG_DATA_DIRS=env["XDG_DATA_HOME"], XDG_CONFIG_DIRS=env["XDG_CONFIG_HOME"])
    return env


def exercise(root, binaries):
    env = isolated_environment(root)
    env["DBUS_SYSTEM_BUS_ADDRESS"] = env["DBUS_SESSION_BUS_ADDRESS"]
    os.environ.clear()
    os.environ.update(env)
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    # Activation children inherit the bus daemon's environment, not this caller's.
    # The daemon was started isolated; publish its newly assigned private address
    # as the system-bus substitute before requesting any activation.
    bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "UpdateActivationEnvironment", GLib.Variant("(a{ss})", (env,)), None,
        Gio.DBusCallFlags.NO_AUTO_START, 5000, None)
    loop = GLib.MainLoop()
    def has_projection():
        return bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "NameHasOwner", GLib.Variant("(s)", ("org.rsynapse.Workspace",)), None,
            Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
    def wake(*_):
        if has_projection():
            loop.quit()
    subscription = bus.signal_subscribe("org.freedesktop.DBus", "org.freedesktop.DBus", "NameOwnerChanged",
        "/org/freedesktop/DBus", "org.rsynapse.Workspace", Gio.DBusSignalFlags.NONE, wake)
    with (root / "projection.log").open("wb") as log:
        projection = subprocess.Popen([str(binaries / "rsynapse-workspace-dbus"), "--backend", "umbriel",
            "--socket", str(root / "absent-compositor.sock")], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            if not has_projection():
                timer = GLib.timeout_add_seconds(10, lambda: (loop.quit(), False)[1])
                loop.run()
                GLib.source_remove(timer)
            assert has_projection()
            before = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                "NameHasOwner", GLib.Variant("(s)", ("org.rsynapse.Steward",)), None,
                Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
            assert not before, "Steward must start through D-Bus activation"
            result = bus.call_sync("org.rsynapse.Steward", "/org/rsynapse/Steward",
                "org.rsynapse.Steward.Session1", "List", None, None, Gio.DBusCallFlags.NONE, 20000, None).unpack()
            assert result == ([],), result
            owner = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                "GetNameOwner", GLib.Variant("(s)", ("org.rsynapse.Steward",)), None,
                Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
            pid = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                "GetConnectionUnixProcessID", GLib.Variant("(s)", (owner,)), None,
                Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
            actual = dict(field.split(b"=", 1) for field in Path(f"/proc/{pid}/environ").read_bytes().split(b"\0") if b"=" in field)
            for key in ("HOME", "XDG_STATE_HOME", "PROJD_STORE_PATH", "LOCUS_RELATIONS_PATH", "STEWARD_SESSION_STORE_PATH", "DBUS_SYSTEM_BUS_ADDRESS"):
                assert actual[key.encode()].decode() == env[key], key
            xml = bus.call_sync(owner, "/org/rsynapse/Steward", "org.freedesktop.DBus.Introspectable",
                "Introspect", None, None, Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
            assert "org.rsynapse.Steward.Associations1" in xml
            assert "org.rsynapse.Steward.Session1" in xml
            print("PASS: rendered activation starts Steward and exposes Associations1 + Session1 on a private bus")
        finally:
            bus.signal_unsubscribe(subscription)
            projection.terminate()
            try:
                projection.wait(timeout=5)
            except subprocess.TimeoutExpired:
                projection.kill()
                projection.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, default=Path.home() / ".local/bin")
    parser.add_argument("--worker", type=Path)
    args = parser.parse_args()
    binaries = args.bin_dir.resolve()
    if args.worker:
        return exercise(args.worker, binaries)
    with tempfile.TemporaryDirectory(prefix="steward-activation-") as temporary:
        root = Path(temporary)
        services = root / "services"
        services.mkdir()
        launchers = root / "bin"
        launchers.mkdir()
        # Track activation children even before they acquire a bus name. Their
        # output must not hold the caller's terminal open after the bus exits.
        for binary in ("rsynapse-steward", "rsynapse-locusd", "rsynapse-projd"):
            launcher = launchers / binary
            launcher.write_text("#!/bin/sh\n"
                f"printf '%s\\n' \"$$\" >> {shlex.quote(str(root / 'pids'))}\n"
                f"exec {shlex.quote(str(binaries / binary))} \"$@\" "
                f">> {shlex.quote(str(root / 'services.log'))} 2>&1\n")
            launcher.chmod(0o700)
        for name in ("Steward", "Locus", "Proj"):
            filename = "org.rsynapse." + name + ".service"
            template = (REPO / "install/dbus-1/services" / (filename + ".in")).read_text()
            (services / filename).write_text(template.replace("@LOCAL_BIN@", str(launchers)))
        config = root / "bus.conf"
        config.write_text('<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" '
            '"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">'
            f'<busconfig><type>session</type><listen>unix:tmpdir={root}</listen><auth>EXTERNAL</auth>'
            f'<servicedir>{services}</servicedir><policy context="default">'
            '<allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/>'
            '<allow own="*"/></policy></busconfig>')
        try:
            subprocess.run(["dbus-run-session", "--config-file=" + str(config), "--", sys.executable,
                str(Path(__file__).resolve()), "--worker", str(root), "--bin-dir", str(binaries)],
                env=isolated_environment(root), check=True, timeout=40)
        finally:
            # The bus is gone, so terminating one service cannot reactivate its
            # dependencies. These PIDs were recorded by our private launchers.
            if (root / "pids").exists():
                for pid in set((root / "pids").read_text().splitlines()):
                    try:
                        os.kill(int(pid), signal.SIGTERM)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    main()
