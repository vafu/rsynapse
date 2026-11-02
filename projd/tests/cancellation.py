"""Cancelled task state: lifecycle, CLI/API parity, selection and restart persistence."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from gi.repository import Gio, GLib

ROOT = Path(__file__).resolve().parents[2]
BIN = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "projd/target")) / "release"
CLI = os.environ.get("PROJ_BIN", str(BIN / "proj"))
DAEMON = os.environ.get("PROJD_BIN", str(BIN / "rsynapse-projd"))
BUS = "org.rsynapse.Proj"
PATH = "/org/rsynapse/Proj"

if "--child" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="projd-cancellation-") as directory:
        env = dict(os.environ, PROJD_STORE_PATH=directory + "/store.sqlite3",
                   PROJD_GOALS_DIR=directory + "/notes", TEST_ROOT=directory,
                   XDG_DATA_HOME=directory, XDG_DATA_DIRS=directory)
        subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--child"], env=env, check=True)
    sys.exit(0)


def cli(*args, check=True):
    return subprocess.run([CLI, *args], text=True, capture_output=True, check=check)


def show(key):
    return json.loads(cli("goal", "show", key).stdout)


connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def ready():
    loop = GLib.MainLoop()
    state = {"ready": False, "timeout": False}

    def appeared(*_):
        state["ready"] = True
        loop.quit()

    def expired():
        state["timeout"] = True
        loop.quit()
        return GLib.SOURCE_REMOVE

    watch = Gio.bus_watch_name_on_connection(connection, BUS, Gio.BusNameWatcherFlags.NONE, appeared, None)
    timeout = GLib.timeout_add_seconds(10, expired)
    loop.run()
    Gio.bus_unwatch_name(watch)
    if not state["timeout"]:
        GLib.source_remove(timeout)
    assert state["ready"], "projd did not acquire its bus name"


base = Path(os.environ["TEST_ROOT"])
project = base / "project"
project.mkdir()
daemon = subprocess.Popen([DAEMON])
try:
    ready()
    project_id = cli("add", str(project)).stdout.strip()
    parent = cli("goal", "add", "parent", "--project", str(project), "--title", "Parent").stdout.strip()
    child = cli("goal", "add", "cancel-me", "--parent", parent, "--title", "Unneeded work").stdout.strip()
    note = Path(cli("goal", "note", child).stdout.strip())
    note.write_text(note.read_text() + "\nKeep the reasoning and evidence.\n")
    original = note.read_bytes()
    identity = json.loads(cli("goal", "instances", child, "--json").stdout)[0]["instance_id"]
    cli("goal", "start", child)
    cli("goal", "cancel", child)
    assert show(child)["status"] == "cancelled" and show(child)["kind"] == "task"
    assert json.loads(cli("goal", "active", "--project", str(project)).stdout) is None
    assert show(parent)["status"] == "planned"  # no cascading state change
    assert note.read_bytes() == original
    assert cli("goal", "active", child, check=False).returncode != 0
    assert cli("goal", "set", child, "--title", "Must not rename", "--active", check=False).returncode != 0
    assert show(child)["title"] == "Unneeded work"
    history = json.loads(cli("goal", "history", child, "--json").stdout)
    assert any(event["kind"] == "status-changed" and event["old_status"] == "in-progress"
               and event["goal"]["status"] == "cancelled" for event in history)
    stats = json.loads(cli("goal", "stats", child, "--json").stdout)
    assert stats["completion_count"] == stats["completed_at_ms"] == 0
    try:
        connection.call_sync(BUS, PATH, "org.rsynapse.Proj.Metrics1", "BeginGoalWork",
                             GLib.Variant("(sss)", (identity, "agent", "test.cancelled")),
                             None, Gio.DBusCallFlags.NONE, 5000, None)
        raise AssertionError("cancelled task accepted a work session")
    except GLib.Error as error:
        assert "Open goal instance not found" in str(error), error

    # Reopening is explicit. Whole-record API updates must also clear selection.
    cli("goal", "start", child)
    record = show(child)
    record["status"] = "cancelled"
    fields = ("id", "date", "title", "kind", "project", "success", "priority", "status",
              "description", "parent_id", "note_path", "project_id")
    connection.call_sync(BUS, PATH, "org.rsynapse.Proj.Manager1", "UpdateGoal",
                         GLib.Variant("((" + "s" * len(fields) + "))", (tuple(record.get(key, "") for key in fields),)),
                         None, Gio.DBusCallFlags.NONE, 5000, None)
    assert show(child)["status"] == "cancelled"
    assert json.loads(cli("goal", "active", "--project", str(project)).stdout) is None
    assert note.read_bytes() == original
    cli("goal", "update", child, "--description", "Cancellation rationale")
    cli("goal", "status", child, "cancelled")
    assert json.loads(cli("goal", "instances", child, "--json").stdout)[0]["instance_id"] == identity

    fixture = base / "import.json"
    fixture.write_text(json.dumps([dict(id="import-cancelled", date="", title="Imported cancellation",
        kind="task", project=str(project), success="", priority="medium", status="cancelled")]))
    cli("goal", "import", str(fixture))
    daemon.terminate()
    daemon.wait(timeout=5)
    daemon = subprocess.Popen([DAEMON])
    ready()
    assert show(child)["status"] == show("import-cancelled")["status"] == "cancelled"
    assert show(child)["description"] == "Cancellation rationale"
    assert json.loads(cli("goal", "active", "--project", str(project)).stdout) is None
    assert json.loads(cli("goal", "stats", child, "--json").stdout)["completion_count"] == 0
    assert note.read_bytes() == original
    print("PASS: cancellation, active selection, history, notes, work eligibility, explicit reopen, API/CLI parity and persistence")
finally:
    daemon.terminate()
    daemon.wait(timeout=5)
