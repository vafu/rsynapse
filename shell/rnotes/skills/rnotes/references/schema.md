# Schema

## Note frontmatter

```yaml
---
tags:
  - task/4f2c9a1e8b7d4c33a6f0d2e5b9c1a7f3
  - note/decision
  - kind/decision
title: Use zbus streams
---
```

## Vocabulary

Task tags (owned by `rnotes` / `task-notes`):

- `task/<immutable-instance-id>` — attaches a note to a task
- `task-main/<immutable-instance-id>` — marks the task's main note

Note roles (`rnotes` vocabulary, free-form `note/*` allowed):

- `note/main`, `note/decision`, `note/research`, `note/plan`, `note/progress`,
  `note/evidence`

Remarked kinds (`kind/*`, fixed set): `fleeting`, `idea`, `research`, `decision`,
`task`, `permanent`, `structure`, `synthesized`, `synthesis-rule`.

## What each layer owns

- projd: task identity, hierarchy, status, history, active selection.
- Remarked: Markdown storage, tags, links, search, triage.
- `rnotes`: the join — creates notes with the task tag, places them in
  `tasks/`, keeps frontmatter canonical, and validates it.
- The agent: note bodies, and any frontmatter changes beyond what rnotes set.

## Rules

- One `task/<instance>` per task note; do not add a second task tag.
- One `note/*` role and one `kind/*` per task note.
- Task notes are never `status/inbox`.
- Use the immutable instance id, not the reusable task key.
- Notes use `tasks/`; other Remarked workflows may use `fleeting/`,
  `permanent/`, and `structure/`.
