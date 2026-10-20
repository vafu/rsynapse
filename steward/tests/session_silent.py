"""Existing-API silent moves and restore-tagged background launch regression."""
import json
import sys
from pathlib import Path

from gi.repository import Gio

BUS = "org.rsynapse.Workspace"
ROOT = "/org/rsynapse/Workspace"
MANAGER = BUS + ".Manager1"


def exercise(root, start, wait, properties, windows, workspace, control, cli):
    probe = str(Path(__file__).with_name("session_restore.py"))
    assert not windows()
    start("observer", [sys.executable, probe, "--probe", "--app-id", "org.rsynapse.Observer"])
    wait(lambda: len(windows()) == 1)
    observer = next(iter(windows()))
    control("FocusWindow", "(ts)", observer)
    origin = properties(ROOT, MANAGER)

    # Exercise the real restore-specific terminal class, without any placement
    # commands that could hide an automatic focus steal by focusing back later.
    scoped = start("background-probe", [sys.executable, probe, "--probe", "--app-id",
        "com.mitchellh.ghostty.rsynapse.r" + "a" * 32])
    wait(lambda: len(windows()) == 2)
    current = properties(ROOT, MANAGER)
    assert current["FocusedWindow"] == origin["FocusedWindow"], (origin, current)
    assert current["FocusedWorkspace"] == origin["FocusedWorkspace"], (origin, current)
    scoped.terminate()
    scoped.wait(timeout=5)
    wait(lambda: set(windows()) == {observer})

    first = cli("restore", "work")
    assert first["restored"] == 2 and not first["issues"], first
    assert properties(ROOT, MANAGER)["FocusedWindow"] == origin["FocusedWindow"]
    restored = set(windows()) - {observer}
    assert len(restored) == 2
    assert len({workspace(window) for window in restored}) == 2

    # An unchanged restore should not select each saved workspace merely to
    # discover that its windows/layout are already in place.
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    visits = []
    def focus_changed(_bus, _sender, _path, _interface, _signal, parameters):
        interface, changed, _invalidated = parameters.unpack()
        if interface == MANAGER and "FocusedWorkspace" in changed:
            visits.append(changed["FocusedWorkspace"])
    subscription = bus.signal_subscribe(BUS, "org.freedesktop.DBus.Properties", "PropertiesChanged",
        ROOT, None, Gio.DBusSignalFlags.NONE, focus_changed)
    try:
        again = cli("restore", "work")
        assert again["reused"] == 2 and not again["issues"], again
        assert all(visit == origin["FocusedWorkspace"] for visit in visits), visits
    finally:
        bus.signal_unsubscribe(subscription)

    # Silent move remains on the source workspace rather than following the
    # window to the destination. The source may become an active empty slot.
    moving = next(window for window in restored if not windows()[window]["Floating"])
    destination = workspace(next(window for window in restored if window != moving))
    control("FocusWindow", "(ts)", moving)
    source = properties(ROOT, MANAGER)["FocusedWorkspace"]
    control("PlaceWindow", "(tssb)", moving, destination, False)
    assert workspace(moving) == destination
    assert properties(ROOT, MANAGER)["FocusedWorkspace"] == source

    # Correcting an actual user move must still return to the observer afterward.
    control("FocusWindow", "(ts)", observer)
    repaired = cli("restore", "work")
    assert repaired["reused"] == 2 and not repaired["issues"], repaired
    assert len({workspace(window) for window in restored}) == 2
    assert properties(ROOT, MANAGER)["FocusedWindow"] == origin["FocusedWindow"]
    assert observer in windows(), "unrelated live window was removed"

    print(json.dumps({"restore_tagged_launch_does_not_focus": True,
        "silent_move_does_not_follow_destination": True, "idempotent_restore_has_no_workspace_tour": True,
        "original_focus_returned": True, "manual_cross_workspace_move_restored": True}))
