#!/usr/bin/env python3
"""Move remarked's active-task marker between tasks.

The active task is the ``active/<project>`` tag on exactly one ``kind/task``
note. remarked owns that tag and enforces the single-active invariant. This
temporary bridge does not touch projd's legacy goal store; it only asks remarked
to move the active tag.

Examples:

    scripts/remarked-active-task.py tasks/new.md
    scripts/remarked-active-task.py --from tasks/old.md tasks/new.md
    scripts/remarked-active-task.py --project 0123abcd --clear

The project defaults to the target task's ``project/<value>`` facet (or
``default`` when the task has no project facet).
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from typing import Any


def executable(env: str, default: str) -> str:
    value = os.environ.get(env) or shutil.which(default)
    if not value:
        raise SystemExit(f"missing executable: {default}")
    return value


def run(command: list[str]) -> str:
    try:
        result = subprocess.run(command, check=True, text=True, capture_output=True)
    except FileNotFoundError as error:
        raise SystemExit(f"missing executable: {command[0]}") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip()
        raise SystemExit(f"{' '.join(command)} failed: {detail}") from error
    return result.stdout.strip()


def json_output(command: list[str]) -> Any:
    output = run(command)
    if not output:
        return None
    try:
        return json.loads(output)
    except json.JSONDecodeError as error:
        raise SystemExit(f"{' '.join(command)} returned invalid JSON: {output}") from error


def remarked(remarked_cli: str, notebook: str | None, args: list[str]) -> list[str]:
    command = [remarked_cli]
    if notebook:
        command.extend(["--notebook", str(Path(notebook).expanduser())])
    command.extend(args)
    return command


def active_task(remarked_cli: str, notebook: str | None, project: str) -> dict[str, Any] | None:
    value = json_output(remarked(remarked_cli, notebook, ["tasks", "active", project]))
    return value if isinstance(value, dict) else None


def task(remarked_cli: str, notebook: str | None, note_id: str) -> dict[str, Any]:
    value = json_output(remarked(remarked_cli, notebook, ["tasks", "get", note_id]))
    if not isinstance(value, dict):
        raise SystemExit(f"remarked did not return task {note_id}")
    return value


def task_id(value: dict[str, Any] | None) -> str | None:
    if not value:
        return None
    candidate = value.get("id")
    return str(candidate) if candidate else None


def project_of(value: dict[str, Any]) -> str:
    return str(value.get("project") or "default")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("note_id", nargs="?", help="task note id to make active")
    parser.add_argument("--project", help="project facet; defaults to the target task's project")
    parser.add_argument("--from", dest="previous", help="require this task to be active first")
    parser.add_argument("--clear", action="store_true", help="clear the project's active task")
    parser.add_argument("--notebook", help="remarked notebook directory or scope path")
    parser.add_argument("--dry-run", action="store_true", help="report the move without applying it")
    args = parser.parse_args()

    if bool(args.note_id) == args.clear:
        parser.error("provide exactly one of NOTE_ID or --clear")
    if args.clear and not args.project:
        parser.error("--clear requires --project")

    remarked_cli = executable("REMARKED_CLI_BIN", "remarked-cli")
    target = None
    project = args.project
    if args.note_id:
        target = task(remarked_cli, args.notebook, args.note_id)
        if target.get("kind") != "task":
            raise SystemExit(f"{args.note_id} is not a task note")
        project = project or project_of(target)
    assert project

    current = active_task(remarked_cli, args.notebook, project)
    current_id = task_id(current)
    if args.previous and current_id != args.previous:
        raise SystemExit(
            f"active task changed: expected {args.previous}, found {current_id or 'none'}"
        )

    if args.clear:
        if not args.dry_run:
            run(remarked(remarked_cli, args.notebook, ["tasks", "deactivate", project]))
            if active_task(remarked_cli, args.notebook, project) is not None:
                raise SystemExit("remarked still reports an active task after deactivation")
        print(json.dumps({"project": project, "previous": current_id, "active": None}, sort_keys=True))
        return 0

    assert target is not None
    target_id = task_id(target)
    if not target_id:
        raise SystemExit(f"remarked did not return a task id for {args.note_id}")

    if current_id != target_id and not args.dry_run:
        run(remarked(remarked_cli, args.notebook, ["tasks", "activate", project, target_id]))
        updated = active_task(remarked_cli, args.notebook, project)
        if task_id(updated) != target_id:
            raise SystemExit(
                f"remarked reports {task_id(updated) or 'no task'} active instead of {target_id}"
            )

    print(
        json.dumps(
            {"project": project, "previous": current_id, "active": target_id},
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
