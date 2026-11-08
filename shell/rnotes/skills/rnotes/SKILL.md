---
name: rnotes
description: "Use for structured work tracking with rnotes: create and manage projd project tasks and subtasks, attach a series of Remarked notes to a task, and read back task state. Trigger when the user asks to create/track tasks, capture decisions or progress against a task, review a task's notes, or work with the project's .notes notebook. Also use when the user refers to rnotes, task notes, or task/subtask checklists."
---

# rnotes

`rnotes` is the CLI for the task-and-notes system: projd owns tasks, Remarked
owns notes, and `rnotes` joins them by tagging each task note with the task's
immutable instance id. This is the primary surface for tracking structured work
and recording the knowledge produced while doing it.

## Routing

Read only what the current step needs:

- `references/workflow.md`: end-to-end loop — pick a task, attach a note, record progress, finish.
- `references/tasks.md`: task creation, subtasks, statuses, active selection, completion.
- `references/notes.md`: note roles, kinds, the printed-path editing workflow, and `check`.
- `references/schema.md`: the task/note tag vocabulary and what rnotes guarantees.

## Always Apply

- Never guess the task. Resolve it first: `rnotes task show`, `rnotes task list`,
  or the explicit `--task KEY`. If nothing is pinned and no task is active,
  choose or create one before writing notes.
- Write task state through `rnotes task ...`; never edit projd's SQLite store.
- Add notes through `rnotes task note add` so they get the task tag. Do not
  hand-create files in `.notes/`.
- A task note is edited by its printed **path**. Read the file, change it, save
  it. Do not rewrite frontmatter tags the tool already set.
- After editing a task note's frontmatter, run `rnotes task note check PATH`.
  Broken YAML silently empties a note's tags, which detaches it from the task.
- Keep prose in notes, status in projd. Do not put completion state in note tags.
- Do not create duplicate main notes; `rnotes task note add` for a main note is
  idempotent and returns the existing path.
