# rnotes

`rnotes` is the shell's CLI counterpart to the **Tasks & Notes** overlay. It
speaks directly to `projd` (`org.rsynapse.Proj`) for project tasks and to the
Remarked daemon (`org.remarked.Service`) for notebooks, notes, Markdown task
lists, search, and triage.

It replaces both shell-facing clients: `remarked-cli` and `remarked-ui`. The
GTK surface now lives in `shell/remarked`, and this binary covers the
project-aware command workflows that used to be split across the two services.

## Usage

```sh
rnotes status
rnotes --format json task list
rnotes task add --title 'Ship the release'
rnotes --task TASK_KEY note capture '# Progress'
rnotes --format json --task TASK_KEY note list
rnotes search --tag proj/rsynapse 'request socket'
rnotes markdown-task list
rnotes triage process-inbox
```

Global options apply to every command:

- `--project PATH` resolves the registered project through projd; it never
  registers a directory. Without it, the current directory is resolved.
- `--task TASK` accepts a projd key, ID, or immutable instance UUID. It must
  belong to the resolved project, and adds the `task/<instance-id>` scope tag.
- `--notebook PATH` skips project resolution and targets a notebook directly.
- `--scope-tag TAG` adds an explicit notebook scope tag (repeatable).
- `--format text|json|jsonl` selects output; diagnostics stay on stderr.

The project notebook is `<project-cwd>/.notes`, matching the shell overlay. Task
instance tags and the main-note tag come from `shell/core/task-notes`, so the CLI
and overlay cannot drift apart.

## Task Commands

`rnotes task ...` is the same domain surface as `proj goal`:

```sh
rnotes --format json task list
rnotes task add --title 'Implement the feature' --description 'Constraints'
rnotes task add --parent GOAL_ID --title 'Verify the implementation'
rnotes task start SUBGOAL_ID
rnotes task status GOAL_ID in-progress
rnotes task complete GOAL_ID
rnotes task cancel GOAL_ID
rnotes task active --clear
```

`task show` includes the parent key, direct children, and the notes attached to
the task's current instance.

## Task Notes

Each task owns a series of notes tagged with its immutable instance id. Notes
live in `<project>/.notes/tasks/` and are created through Remarked, then
normalized so they always carry one role, one kind, the task tag, and no
`status/inbox`.

```sh
rnotes task start KEY                       # pin the implicit task
rnotes task checkpoint KEY                  # or pin without changing status
rnotes task note add                        # main note (idempotent)
rnotes task note add --role decision --kind decision --title 'Choose a design'
rnotes task note add --role progress --title 'Retry logic' --body 'Tried X'
rnotes --format json task note list
rnotes task note path --role main
rnotes task note show NOTE_ID
rnotes task note check PATH [--fix]
rnotes task note delete NOTE_ID --yes
rnotes task uncheckpoint
```

`add`, `path`, and `show` print an absolute path; that path is the edit
interface. Edit the Markdown directly. If you change frontmatter, run
`task note check` — broken YAML does not fail `zk`, it silently reports empty
tags, which detaches the note from its task. `--fix` repairs safely when YAML
parses and refuses otherwise.

Implicit task resolution: `--task`, else `.rnotes-task` in the project root,
else the project's active task.

## Agent Session Pointers

The skill is installed globally to `${CODEX_HOME:-~/.codex}/skills/rnotes`, so it
applies to every project. Two optional session pointers are available:

- A global `developer_instructions` entry in `~/.codex/config.toml` (trust-free)
  pointing at the skill. This is what the installer's docs recommend.
- `scripts/session-context.sh`: a `SessionStart` hook that injects the notebook
  path only inside projects that have `.notes/.zk`. Run `install/local.sh` to
  publish it as `~/.local/bin/rnotes-session-context`.

Codex gates each hook handler on its own trust hash in `config.toml`
(`[hooks.state."<file>:session_start:<group>:<handler>"]`). Trust is per handler,
not per event, so adding this hook does not disturb existing hooks.
`install/local.sh` registers the hook and persists its trust automatically via
`install/bin/rnotes-hook-install.py`, which asks the Codex app server for the
authoritative `currentHash` rather than guessing it. Re-run the installer after
editing the hook script, since its hash changes.

Markdown checkbox tasks inside the notebook remain available separately:

```sh
rnotes markdown-task list --status open
rnotes markdown-task capture 'Write the migration note'
rnotes markdown-task complete TASK_REF
```

Keeping them under `markdown-task` avoids overloading "task" across the two
models. Project tasks own completion in projd; Markdown checkbox state owns
completion inside notebook task lists.

## Notes, Search, And Triage

```sh
rnotes note capture 'Design finding'
rnotes --format json note list --inbox
rnotes note get NOTE_ID --body
rnotes note context NOTE_ID
rnotes note replace NOTE_ID --revision REV 'Updated body'
rnotes note delete NOTE_ID --revision REV --yes
rnotes note links NOTE_ID --direction both
rnotes tags
rnotes search --mode lexical --limit 5 'camera calibration'
rnotes triage policy
rnotes triage list --ids
rnotes triage commit NOTE_ID --kind decision --tag topic/design
```

Triage commits still validate kinds against the service policy and reject
un-namespaced or `kind/`/`status/` capture tags, matching the former CLI.

## Boundary

`rnotes` composes services; it does not own their state. It creates notes through
Remarked and may write note files for creation, placement, normalization, and
`check --fix`; it never owns note prose or task state. It must not import
Remarked internals or duplicate projd's task rules. The `task-notes` crate owns
the shared naming convention, and both services remain independent.
