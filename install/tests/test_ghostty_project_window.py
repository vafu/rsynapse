"""Black-box launch checks, without acquiring desktop services or opening windows."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[1] / "bin" / "ghostty-project-window"
MOCK = r'''
import json, os, sys
from pathlib import Path

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
case = os.environ["LAUNCH_CASE"]
project = os.environ["PROJECT_ROOT"]
cwd = os.environ["PROJECT_CWD"]
if tool == "busctl":
    assert args[:2] == ["--user", "--json=short"], args
    if case == "disconnected":
        sys.exit(1)
    if args[2:4] == ["call", "org.rsynapse.Proj"]:
        if case == "removed":
            sys.exit(1)
        cwd = "/nonexistent/rsynapse-test-cwd" if case == "missing-cwd" else cwd
        if case == "project-id":
            assert args[4:] == ["/org/rsynapse/Proj", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects"], args
            result = {"type": "a{oa{sa{sv}}}", "data": [{"/unrelated/path": {
                "org.rsynapse.Proj.Project1": {"Id": {"type": "s", "data": "project-uuid"},
                    "Cwd": {"type": "s", "data": cwd}}}}]}
        else:
            assert args[4:] == ["/org/rsynapse/Proj", "org.rsynapse.Proj.Manager1", "ResolveProject", "s", project], args
            result = {"type": "ssssss", "data": ["project-uuid", "name", cwd, "checkout", "", ""]}
    elif args[2] == "get-property":
        assert args[3] == "org.rsynapse.Workspace", args
        if args[-1] == "FocusedWorkspace":
            assert args[4:6] == ["/org/rsynapse/Workspace", "org.rsynapse.Workspace.Manager1"], args
            # Deliberately unrelated to the actual opaque ID.
            result = {"type": "ao", "data": ["/org/rsynapse/Workspace/Workspaces/unrelated"]}
        else:
            assert args[4:] == ["/org/rsynapse/Workspace/Workspaces/unrelated", "org.rsynapse.Workspace.Workspace1", "Id"], args
            result = {"type": "s", "data": "DP-3:opaque/α"}
    else:
        assert args[2:] == ["call", "org.rsynapse.Locus", "/org/rsynapse/Locus", "org.rsynapse.Locus.Relations1",
            "Targets", "a{ss}s", "3", "type", "stable-key", "kind", "org.rsynapse.workspace.id",
            "id", "DP-3:opaque/α", "org.rsynapse.workspace.project"], args
        target = {"type": "stable-key", "kind": "org.rsynapse.project.path", "id": project}
        if case == "project-id":
            target.update(kind="org.rsynapse.project.id", id="project-uuid")
        result = {"type": "aa{ss}", "data": [[] if case == "unbound" else [target]]}
    print(json.dumps(result))
elif tool == "proj":
    raise AssertionError("desktop PATH may contain an obsolete proj script; use D-Bus")
elif tool == "ghostty":
    print(json.dumps(args))
else:
    raise AssertionError(tool)
'''


class GhosttyProjectWindow(unittest.TestCase):
    def test_project_cwd_and_fallbacks(self):
        with tempfile.TemporaryDirectory(prefix="ghostty-launch-") as temporary:
            root = Path(temporary)
            saved_cwd = root / "project with spaces" / "saved subdirectory"
            saved_cwd.mkdir(parents=True)
            for name in ("busctl", "proj", "ghostty"):
                executable = root / name
                executable.write_text(f"#!{sys.executable}\n{MOCK}")
                executable.chmod(0o755)
            for case in ("project-path", "project-id", "unbound", "disconnected", "removed", "missing-cwd"):
                with self.subTest(case=case):
                    environment = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                        LAUNCH_CASE=case, PROJECT_ROOT=str(saved_cwd.parent), PROJECT_CWD=str(saved_cwd))
                    result = subprocess.run(["bash", str(HELPER)], env=environment,
                        text=True, capture_output=True, check=True)
                    expected = str(saved_cwd) if case.startswith("project-") else "/tmp"
                    self.assertEqual(json.loads(result.stdout), [
                        "--window-inherit-working-directory=false",
                        f"--working-directory={expected}", "+new-window"])


if __name__ == "__main__":
    unittest.main()
