"""Scenario used by the isolated session_restore.py harness."""
import json
import os
import socket
import sys
from pathlib import Path

BUS = "org.rsynapse.Workspace"
ROOT = "/org/rsynapse/Workspace"
MANAGER, WORKSPACE, OUTPUT, WINDOW = [BUS + "." + name + "1" for name in ("Manager", "Workspace", "Output", "Window")]


def exercise(root, start, wait, properties, windows, workspace, control, bind, cli, restart_steward):
    def request(command, argument):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(os.environ["UMBRIEL_SOCKET"])
            stream.sendall(json.dumps({"cmd": command, "arg": argument}).encode() + b"\n")
            reply = json.loads(stream.makefile().readline())
            assert "ok" in reply, reply
            return reply["ok"]

    request("output-create", "group-hotplug 800x600@30")
    def group_output_present():
        for path in properties(ROOT, MANAGER)["Workspaces"]:
            outputs = properties(path, WORKSPACE)["Output"]
            if outputs and properties(outputs[0], OUTPUT)["Name"] == "group-hotplug":
                return True
        return False
    wait(group_output_present)
    expected, surviving = {}, {}
    scenarios = [("HEADLESS-1", 1, "project-a"), ("HEADLESS-2", 1, "project-a"),
        ("group-hotplug", 1, "project-b"), ("group-hotplug", 2, "project-c"),
        ("group-hotplug", 3, "project-b"), ("group-hotplug", 4, None)]
    for step, (output, index, project) in enumerate(scenarios):
        wait(lambda: any(properties(path, WORKSPACE)["Index"] >= index and
            properties(path, WORKSPACE)["Output"] and
            properties(properties(path, WORKSPACE)["Output"][0], OUTPUT)["Name"] == output
            for path in properties(ROOT, MANAGER)["Workspaces"]))
        selected = control("SelectWorkspace", "(tsu)", output, index)
        before = set(windows())
        start("hotplug-client-" + str(step), [sys.executable, str(Path(__file__).with_name("session_restore.py")), "--probe"])
        wait(lambda: len(windows()) > len(before))
        window = next(iter(set(windows()) - before))
        control("PlaceWindow", "(tssb)", window, selected, False)
        if project:
            bind(window, project)
        expected[window] = str(root / project) if project else None
        if output != "group-hotplug":
            surviving[window] = workspace(window)
    focused = next(iter(surviving))
    control("FocusWindow", "(ts)", focused)
    saved = cli("save", "pre-hotplug")
    assert len(saved["workspaces"]) == 6, saved
    assert sum(len(w["windows"]) for w in saved["workspaces"]) == 6
    before_windows = set(windows())
    request("output-destroy", "group-hotplug")

    def grouped():
        if set(windows()) != before_windows:
            return False
        homes = {window: workspace(window) for window in before_windows}
        return len(set(homes.values())) == 6 and all(homes[window] == home for window, home in surviving.items())
    wait(grouped)
    from gi.repository import Gio, GLib
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    def bindings():
        rows = bus.call_sync("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", "List",
            GLib.Variant("(s)", ("org.rsynapse.workspace.project",)), None, Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]
        return {row[0]["id"]: row[2]["id"] for row in rows}
    wait(lambda: all(bindings().get(workspace(window)) == project for window, project in expected.items()))
    checkpoint = cli("show", "before-output-change")
    assert checkpoint["workspaces"] == saved["workspaces"], "pre-removal snapshot was overwritten"
    last = cli("show", "last")
    assert sum(len(w["windows"]) for w in last["workspaces"]) == 6
    assert len([w for w in last["workspaces"] if w["windows"]]) == 6, last
    # Preferences and same-compositor identities survive the policy daemon's
    # restart; the compositor/projection and application instances stay alive.
    restart_steward()
    # Reconnect the original monitor and disable the fallback output, as when
    # docking a laptop and closing its lid. Retained empty workspace objects
    # must not create duplicate logical identities.
    evacuated = next(window for window in expected if window not in surviving)
    fallback = properties(properties(windows()[evacuated]["Workspace"][0],WORKSPACE)["Output"][0],OUTPUT)["Name"]
    request("output-create","group-hotplug 800x600@30")
    wait(group_output_present)
    request("msg","output-disable:"+fallback)
    def original_monitor(window):
        return properties(properties(windows()[window]["Workspace"][0],WORKSPACE)["Output"][0],OUTPUT)["Name"]
    wait(lambda: all(original_monitor(window)=="group-hotplug" for window in expected if window not in surviving)
        and len({workspace(window) for window in expected})==6)
    request("msg","output-enable:"+fallback)
    wait(lambda: all(original_monitor(window)==("group-hotplug" if window not in surviving else
        ("HEADLESS-1" if window==next(iter(surviving)) else "HEADLESS-2")) for window in expected))
    cli("save","after-reconnect")
    # An ordinary user move after the monitor handoff is intent, not a topology
    # loss. It must not be undone by the preservation policy.
    moved, target = [window for window, project in expected.items() if project and project.endswith("project-b")][0], next(window for window, project in expected.items() if project and project.endswith("project-c"))
    destination = workspace(target)
    control("PlaceWindow", "(tssb)", moved, destination, False)
    manual = cli("save", "after-manual-move")
    assert workspace(moved) == destination
    assert len([w for w in manual["workspaces"] if w["windows"]]) == 5
    home = workspace(focused)
    control("FocusWindow", "(ts)", focused)
    request("msg", "window-move-to-scratchpad")
    (root / "scratchpad-inspection.json").write_text(json.dumps({"focused":focused,
        "native_windows":request("windows", ""), "native_workspaces":request("workspaces", "")}, indent=2))
    wait(lambda: not windows()[focused]["Workspace"])
    request("msg", "scratchpad-toggle")
    request("msg", "window-restore-from-scratchpad")
    wait(lambda: windows()[focused]["Workspace"] and workspace(focused) == home)
    print(json.dumps({"hotplug_groups": True, "same_project_distinct_workspaces": True,
        "surviving_destination_bindings_preserved": True, "existing_windows_only": True,
        "pre_removal_checkpoint_preserved": True, "manual_moves_respected": True,
        "unassigned_workspace_preserved": True, "scratchpad_updates_remain_live": True,
         "preferred_outputs_returned": True, "disabled_fallback_identity_retired": True,
        "preferences_survive_steward_restart": True,
        "populated_workspaces_after_repair": 6}))
