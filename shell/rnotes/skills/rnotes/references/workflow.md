# Workflow

## Pin the task once

```sh
rnotes task start KEY        # marks in-progress and selects it as the project's active task
# or, without changing status:
rnotes task checkpoint KEY
```

With a checkpoint or active task set, task-note commands work without repeating
the key:

```sh
rnotes task note add
rnotes task note list
```

Clear the pin with `rnotes task uncheckpoint`. An explicit `--task KEY` always
wins over both.

## Main note

Every task has one main note holding description, context, and running log.

```sh
PATH=$(rnotes task note add)
```

This creates `tasks/<id>.md` with the task title and description, tagged
`task/<instance>`, `note/main`, `kind/fleeting`, and `task-main/<instance>`.
Running it again returns the same path and creates nothing.

## Series notes

Add a note whenever the work produces something durable:

| Role | Use for |
| --- | --- |
| `decision` | A choice, with the alternatives and why |
| `research` | Findings from reading, experiments, or the web |
| `plan` | A concrete approach before implementing |
| `progress` | A checkpoint, result, or status the user will want later |
| `evidence` | Proof a task criterion is met |

```sh
rnotes task note add --role decision --kind decision --title 'Use zbus streams' --body '...'
rnotes task note add --role research --kind research --title 'Stream APIs' --body '...'
```

`--kind` sets the Remarked kind (`fleeting`, `idea`, `research`, `decision`,
`task`, ...). `--title` sets frontmatter. `--body` accepts text, `-` for stdin,
or is omitted for a heading-only note.

## Edit by path

Commands that create or locate a note print its path. Edit that file directly:

```sh
rnotes task note add --role progress --title 'Retry logic'
# edit the printed path with your file tools
rnotes task note check /path/to/note.md
```

Read a note back through `rnotes task note show NOTE_ID` (path, revision, tags,
body) or `rnotes --format json task note list`.

## Close out

1. Verify the task's success criterion.
2. Record evidence in an `evidence` note.
3. `rnotes task complete KEY`.

Completing a task does not complete its children, and completing children does
not complete the parent.
