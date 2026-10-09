"""Private-bus regression for legacy project bindings, icons and agent references."""
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / "steward/target/release"
LOCUS = os.environ.get("LOCUS_BIN", str(pathlib.Path.home() / ".local/bin/locusd"))

if "--child" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="steward-model-migration-") as temp:
        base = pathlib.Path(temp)
        for name in ["primary", "worktree", "directory"]:
            (base / name).mkdir()
        status = dict(staged=0, unstaged=0, untracked=0, ahead=0, behind=0,
                      merging=False, rebasing=False, stashes=0)
        goal = dict(id="goal", date="2026-10-07", title="Saved title", kind="outcome",
                    project=str(base / "worktree"), success="Saved criterion",
                    priority="high", status="in-progress")
        db = sqlite3.connect(base / "store.sqlite3")
        db.execute("CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id))")
        entries = [
            ("project", "p", dict(id="p", name="Custom primary", root_path=str(base / "primary"))),
            ("checkout", "a", dict(id="a", project_id="p", root_path=str(base / "primary"), branch="main", git_status=status)),
            ("checkout", "b", dict(id="b", project_id="p", root_path=str(base / "worktree"), branch="feature", git_status=status)),
            ("project", "dir", dict(id="dir", name="Directory", root_path=str(base / "directory"))),
            ("checkout", "d", dict(id="d", project_id="dir", root_path=str(base / "directory"), branch="", git_status=status)),
            ("context", "ctx", dict(id="ctx", project_id="p", checkout_id="b", cwd=str(base / "worktree"), relative_cwd=".")),
            ("goal", "2026-10-07/goal", goal),
        ]
        for kind, id, value in entries:
            db.execute("INSERT INTO records VALUES(?,?,?)", (kind, id, json.dumps(value)))
        db.commit()
        db.close()
        env = dict(os.environ, TEST_ROOT=temp, PROJD_STORE_PATH=str(base / "store.sqlite3"),
                   LOCUS_RELATIONS_PATH=str(base / "relations.json"),
                   XDG_DATA_HOME=temp, XDG_DATA_DIRS=temp)
        subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--child"], env=env, check=True)
    sys.exit(0)

def call(service, path, interface, method, *args):
    result = subprocess.check_output(["busctl", "--user", "--json=short", "call",
                                     service, path, interface, method, *args], text=True)
    return json.loads(result)["data"]

def locus(method, *args):
    return call("org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1", method, *args)[0]

def mapping(values):
    return [str(len(values)), *[str(v) for pair in values.items() for v in pair]]

def key(kind, id):
    return dict(type="stable-key", kind=kind, id=id)

def bind(subject, relation, target, metadata=None, persist=True):
    return locus("SetWithPersistence", "a{ss}sa{ss}a{ss}b", *mapping(subject), relation,
                 *mapping(target), *mapping(metadata or {}), str(persist).lower())

def wait(fn):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            result = fn()
            if result:
                return result
        except subprocess.CalledProcessError:
            pass
        time.sleep(.02)
    raise AssertionError("Private service did not reach expected state")

def projects():
    return json.loads(subprocess.check_output([str(BIN / "proj"), "project", "list", "--json"], text=True))

base = pathlib.Path(os.environ["TEST_ROOT"])
daemons = [subprocess.Popen([LOCUS]), subprocess.Popen([str(BIN / "projd")])]
try:
    wait(lambda: projects())
    wait(lambda: subprocess.run(["busctl", "--user", "status", "org.rsynapse.Locus"], capture_output=True).returncode == 0)
    ws = lambda id: key("org.rsynapse.niri.workspace.id", str(id))
    relation = "org.rsynapse.workspace.project"
    override = "org.rsynapse.workspace.icon-override"
    for workspace, checkout, directory, persist in [(1, "a", "primary", True), (2, "b", "worktree", False), (3, "d", "directory", True)]:
        bind(ws(workspace), relation, key("org.rsynapse.project.path", str(base / directory)),
             {"project-id": "dir" if workspace == 3 else "p", "checkout-id": checkout, "context-id": "ctx"}, persist)
    bind(ws(1), override, key("org.rsynapse.icon.glyph", "PRIMARY"))
    bind(ws(2), "org.rsynapse.workspace.name", key("org.rsynapse.workspace.name", "coding"), {"source": "manual"})
    bind(key("org.rsynapse.niri.workspace.name", "coding"), override, key("org.rsynapse.icon.glyph", "WORKTREE"))
    call("org.rsynapse.Proj", "/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "SetProjectIcon", "ss", "dir", "PRESET")
    bind(ws(3), override, key("org.rsynapse.icon.glyph", "CONFLICT"))
    old_agent = dict(type="dbus-object", bus="session", service="org.rsynapse.Proj",
                     path="/org/rsynapse/Proj/Projects/n70", interface="org.rsynapse.Proj.Project1")
    bind(old_agent, "org.rsynapse.project.agent", key("org.rsynapse.agent.session.id", "codex/session"))
    before_goals = subprocess.check_output([str(BIN / "proj"), "goal", "list", "--all", "--json"])
    daemons.append(subprocess.Popen([str(BIN / "steward"), "associations"]))
    wait(lambda: all(p["icon"] for p in projects() if p["checkout_id"]))
    migrated = projects()
    primary = next(p for p in migrated if p["id"] == "p")
    worktree = next(p for p in migrated if p["checkout_id"] == "b")
    directory = next(p for p in migrated if p["id"] == "dir")
    assert primary["name"] == "Custom primary" and primary["icon"] == "PRIMARY"
    assert worktree["id"] != "p" and worktree["icon"] == "WORKTREE" and worktree["icon_origin"] == "manual"
    assert directory["name"] == "Directory" and directory["checkout_id"] == ""
    assert directory["icon"] == "PRESET" and directory["icon_origin"] == "manual"
    states = locus("ListWithPersistence", "s", relation)
    assert {r[0]["id"]: persist for r, persist in states} == {"1": True, "2": False, "3": True}
    assert all("context-id" not in r[3] for r, _ in states)
    assert next(r for r, _ in states if r[0]["id"] == "2")[3]["project-id"] == worktree["id"]
    assert next(r for r, _ in states if r[0]["id"] == "3")[3]["checkout-id"] == ""
    remaining = wait(lambda: (lambda rows: rows if len(rows) == 1 else None)(locus("List", "s", override)))
    assert remaining[0][0] == ws(3) and remaining[0][2]["id"] == "CONFLICT"
    agent = wait(lambda: locus("ListWithPersistence", "s", "org.rsynapse.project.agent"))
    assert len(agent) == 1 and agent[0][0][0]["path"] == "/org/rsynapse/Proj/Projects/pp" and agent[0][1]
    assert subprocess.check_output([str(BIN / "proj"), "goal", "list", "--all", "--json"]) == before_goals
    assert locus("List", "s", "org.rsynapse.workspace.name")[0][2]["id"] == "coding"
    print("PASS: split-project bindings, persistence flags, ID/name-keyed icons, non-Git association, agent paths and complete goals preserved")
finally:
    for daemon in reversed(daemons):
        daemon.terminate()
        daemon.wait(timeout=5)
