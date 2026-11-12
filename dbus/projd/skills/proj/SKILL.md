---
name: proj
description: Use when working in a registered project, maintaining project registration, metadata, labels, or project-scoped task context. References $remarked-cli for general note and task operations.
---

# Proj — Project Context

`projd` owns project registration and metadata. For notebook operations, use the
`$remarked-cli` skill; this skill only adds project context and the convention
that a project's active task is a remarked task identified by the project id.

## Resolve the Project

```sh
proj metadata --json
proj root
proj project list --json
proj checkout list --json
```

Each Git checkout/worktree is a separate project. `metadata --json` returns the
stable project `id`, name, checkout, labels, and paths. Read/refresh operations
do not register directories.

Register an intentionally chosen project explicitly:

```sh
proj add /absolute/checkout --name "Project name"
proj update /absolute/checkout
```

Do not register incidental directories, dependencies, or every directory found
during a search. `proj root` and `proj metadata` only resolve existing
registrations.

## Project Labels

```sh
proj project labels get --json
proj project labels add bug 'team name' --project /absolute/project --json
proj project labels remove bug --project-id PROJECT_ID --json
```

Without a target flag, labels resolve the current directory's registered
project and never register it. `--project PATH` and `--project-id ID` are
mutually exclusive. Label edits are atomic; labels trim outer whitespace,
reject empty/control strings, and preserve case and internal spaces.
`proj metadata --json` includes `labels`.

## Project-Scoped Note and Task Tracking

Use `$remarked-cli` for the actual notebook operations. For project work:

1. Resolve `PROJECT_ID` with `proj metadata --json`.
2. Check the project's selected task:

   ```sh
   remarked-cli tasks active PROJECT_ID
   ```

3. List and read relevant tasks:

   ```sh
   remarked-cli tasks list
   remarked-cli tasks get NOTE_ID
   remarked-cli notes read NOTE_ID
   ```

4. Create tasks with the project facet, subtasks as child-note links, and keep
   status in `status/*` tags:

   ```sh
   remarked-cli tasks create "Implement feature" --project PROJECT_ID
   remarked-cli tasks subtask PARENT_NOTE_ID "Verify behavior" --project PROJECT_ID
   remarked-cli tasks status NOTE_ID --revision REV in-progress
   remarked-cli tasks activate PROJECT_ID NOTE_ID
   ```

5. Put plans, decisions, blockers, progress, and verification evidence in the
   task note body. Task notes are the durable record; projd stores no task rows.
6. Complete only verified work. `tasks deactivate PROJECT_ID` stops tracking
   without completing the task.

Do not recreate projd goals or copy task state into project metadata. Projd owns
projects; remarked owns notes and tasks.

## Temporary Active-Task Bridge

Until projd delegates active selection through the script system, the bridge
script moves remarked's active tag without touching projd:

```sh
scripts/remarked-active-task.py tasks/new.md
scripts/remarked-active-task.py --from tasks/old.md tasks/new.md
scripts/remarked-active-task.py --project PROJECT_ID --clear
```

The project defaults to the target task's `project/<value>` facet. The script
delegates to remarked and never writes task state into projd.
