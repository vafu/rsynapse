"""Contract tests for the temporary remarked active-task bridge."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "remarked-active-task.py"


class ActiveTaskScript(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="remarked-active-task-"))
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.state = self.root / "state.json"
        self.log = self.root / "calls.jsonl"
        self.log.write_text("")
        self.state.write_text(
            json.dumps(
                {
                    "active": {"default": "tasks/old.md"},
                    "tasks": {
                        "tasks/old.md": {"id": "tasks/old.md", "kind": "task", "project": "default"},
                        "tasks/new.md": {"id": "tasks/new.md", "kind": "task", "project": "default"},
                        "tasks/other.md": {"id": "tasks/other.md", "kind": "task", "project": "other"},
                        "notes/x.md": {"id": "notes/x.md", "kind": "fleeting", "project": "default"},
                    },
                }
            )
        )
        self.write_executable("remarked-cli", REMARKED)

    def write_executable(self, name, source):
        path = self.bin / name
        path.write_text(f"#!{sys.executable}\n{source}")
        path.chmod(0o755)

    def run_script(self, *args, check=True):
        env = dict(
            os.environ,
            PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
            BRIDGE_STATE=str(self.state),
            BRIDGE_LOG=str(self.log),
        )
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            check=check,
            text=True,
            capture_output=True,
            env=env,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_activate_infers_project_from_task_and_uses_remarked_owner(self):
        result = self.run_script("tasks/new.md")
        self.assertEqual(
            json.loads(result.stdout),
            {"project": "default", "previous": "tasks/old.md", "active": "tasks/new.md"},
        )
        self.assertEqual(
            self.calls(),
            [
                ["tasks", "get", "tasks/new.md"],
                ["tasks", "active", "default"],
                ["tasks", "activate", "default", "tasks/new.md"],
                ["tasks", "active", "default"],
            ],
        )
        self.assertEqual(json.loads(self.state.read_text())["active"]["default"], "tasks/new.md")

    def test_explicit_project_and_notebook_are_forwarded(self):
        self.run_script("--notebook", "/notes", "--project", "other", "tasks/other.md")
        self.assertEqual(
            self.calls(),
            [
                ["--notebook", "/notes", "tasks", "get", "tasks/other.md"],
                ["--notebook", "/notes", "tasks", "active", "other"],
                ["--notebook", "/notes", "tasks", "activate", "other", "tasks/other.md"],
                ["--notebook", "/notes", "tasks", "active", "other"],
            ],
        )

    def test_explicit_project_overrides_task_facet(self):
        self.run_script("--project", "default", "tasks/other.md")
        self.assertEqual(
            self.calls()[:3],
            [
                ["tasks", "get", "tasks/other.md"],
                ["tasks", "active", "default"],
                ["tasks", "activate", "default", "tasks/other.md"],
            ],
        )

    def test_expected_previous_task_guards_concurrent_change(self):
        result = self.run_script("--from", "tasks/stale.md", "tasks/new.md", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("active task changed: expected tasks/stale.md", result.stderr)
        self.assertNotIn("tasks/activate", json.dumps(self.calls()))

    def test_clear_removes_active_task(self):
        result = self.run_script("--project", "default", "--clear")
        self.assertEqual(
            json.loads(result.stdout),
            {"project": "default", "previous": "tasks/old.md", "active": None},
        )
        self.assertNotIn("default", json.loads(self.state.read_text())["active"])

    def test_dry_run_does_not_mutate(self):
        result = self.run_script("tasks/new.md", "--dry-run")
        self.assertEqual(json.loads(result.stdout)["active"], "tasks/new.md")
        self.assertEqual(json.loads(self.state.read_text())["active"]["default"], "tasks/old.md")
        self.assertEqual(self.calls()[-1], ["tasks", "active", "default"])

    def test_rejects_non_task_note(self):
        result = self.run_script("notes/x.md", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not a task note", result.stderr)

    def test_clear_requires_project(self):
        result = self.run_script("--clear", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--clear requires --project", result.stderr)

    def test_previous_guard(self):
        result = self.run_script("--from", "tasks/old.md", "tasks/new.md")
        self.assertEqual(json.loads(result.stdout)["previous"], "tasks/old.md")


REMARKED = """
import json, os, sys
state = json.load(open(os.environ["BRIDGE_STATE"]))
args = sys.argv[1:]
open(os.environ["BRIDGE_LOG"], "a").write(json.dumps(args) + "\\n")
if args[:1] == ["--notebook"]:
    args = args[2:]
if args[:2] == ["tasks", "active"]:
    active = state["active"].get(args[2])
    print(json.dumps(state["tasks"].get(active)) if active else "null")
elif args[:2] == ["tasks", "get"]:
    task = state["tasks"].get(args[2])
    if task is None:
        print("not found", file=sys.stderr)
        raise SystemExit(1)
    print(json.dumps(task))
elif args[:2] == ["tasks", "activate"]:
    state["active"][args[2]] = args[3]
    json.dump(state, open(os.environ["BRIDGE_STATE"], "w"))
elif args[:2] == ["tasks", "deactivate"]:
    state["active"].pop(args[2], None)
    json.dump(state, open(os.environ["BRIDGE_STATE"], "w"))
else:
    print("unexpected", args, file=sys.stderr)
    raise SystemExit(2)
"""

