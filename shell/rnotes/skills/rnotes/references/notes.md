# Notes

## Create

```sh
rnotes task note add                                   # main note
rnotes task note add --role decision --kind decision --title 'Use zbus streams'
rnotes task note add --new --role progress --title 'Second progress note'
```

Notes are written to `<project>/.notes/tasks/`. rnotes creates them through the
Remarked service and then normalizes their frontmatter, so every task note has:

- `task/<instance>` — task membership (immutable instance id)
- `note/<role>` — one role
- `kind/<kind>` — exactly one Remarked kind
- no `status/inbox`

The main note also carries `task-main/<instance>` so the shell overlay finds it.

## Read

```sh
rnotes task note list
rnotes --format json task note list --role decision
rnotes task note path --role main
rnotes task note show NOTE_ID
```

## Edit

`add`, `path`, and `show` print an absolute path. Edit that file directly and
keep the existing frontmatter tags intact. For body-only changes this needs no
extra step.

If you change frontmatter, validate it:

```sh
rnotes task note check PATH
```

Checks: YAML parses, `task/<instance>` matches, exactly one `kind/*`, at least
one `note/*`, and warns if `status/inbox` is back. Exit code 3 means a real
problem.

Safe automatic repair:

```sh
rnotes task note check PATH --fix
```

`--fix` only runs when YAML parses. It adds the missing task tag, a default role
and kind, and removes `status/inbox`. It never invents a role you did not set and
never rewrites broken YAML — fix that by hand first.

## Delete

```sh
rnotes task note delete NOTE_ID --yes
```

Deleting a task retains its notes and history. Remove or repoint notes
deliberately before deleting a task.
