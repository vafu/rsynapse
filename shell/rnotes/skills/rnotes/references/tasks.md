# Tasks

Tasks are projd goals. Each Git checkout/worktree is its own project.

## Inspect before creating

```sh
rnotes --format json task list
rnotes task show KEY
```

`task show` returns the task, its parent key, direct child tasks, and the notes
attached to its current instance.

## Create

```sh
rnotes task add --title 'Ship the release' --description 'Constraints and scope'
rnotes task add --parent PARENT_KEY --title 'Build the artifact'
rnotes task set release --title 'Ship v1'          # idempotent by stable key
```

`--kind` is `task` (default), `outcome`, or `habit`. Outcomes need a project and
a `--success` criterion; habits are personal and are never completed from
tooling activity.

## Status

```sh
rnotes task start KEY
rnotes task status KEY in-progress
rnotes task status KEY planned
rnotes task status KEY deferred
rnotes task complete KEY
rnotes task cancel KEY
rnotes task update KEY --title 'Refined title' --description 'Refined scope'
```

States: `planned`, `in-progress`, `completed`, `deferred`, `cancelled`.
Cancellation is terminal but is not completion; reopen explicitly before
tracking again.

## Active task

```sh
rnotes task active KEY
rnotes task active
rnotes task active --clear
```

The active task drives the shell bar and the implicit task for notes. A
checkpoint file (`.rnotes-task` in the project root) overrides it for the CLI;
add it to your ignore rules.
