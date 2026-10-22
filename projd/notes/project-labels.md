# Project labels implementation and verification

Assigned goal: `6d70c454fd634a81bfd003281e130afa`.
Parent goal: `393e09619fa34010b9d01a945c5820ff` (selection owned by parent agent).

## Progress — 2026-10-21

Implementation and verification are complete within `projd/`.
Assigned goal status was set to `completed` after verification; active selection
and parent status were not changed.

- Added read-only Project1.Labels (`as`) and Manager1.AddProjectLabels /
  RemoveProjectLabels (`s, as -> as`); old ProjectInfo signatures are preserved.
- Persisted exact sorted label sets as project-owned `project-labels` records
  keyed by project UUID in the existing SQLite table. Missing records mean `[]`.
- Validated whole batches, rejected empty/control strings, trimmed outer Unicode
  whitespace and preserved spelling/internal spaces. Mutations share the existing
  lock, check project existence transactionally, and avoid writes/signals on no-op.
- Emitted Labels PropertiesChanged and root Changed("project", id) after commit.
  ObjectManager includes labels initially and after restart. Removal cleans labels
  in the project deletion transaction; old identity cannot be resurrected.
- Added CLI `proj project labels get|add|remove`, multiple add/remove operands,
  mutually exclusive directory/UUID targets, current-directory resolution without
  registration, JSON output, and labels in metadata JSON.
- Updated README, local AGENTS and agent usage instructions.

## Verification evidence

All commands ran from `/tmp/opencode`. Cargo used the approved isolated cache,
shared target, two jobs, no incremental compilation, offline and locked mode:

```sh
env CARGO_HOME=/home/vfuchedzhy/proj/rsynapse/steward/.cargo-home \
  CARGO_TARGET_DIR=/home/vfuchedzhy/proj/rsynapse/steward/target \
  CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
  cargo test --offline --locked \
  --manifest-path /home/vfuchedzhy/proj/rsynapse/projd/Cargo.toml
```

Result: **15 passed, 0 failed** (1 CLI + 14 service/store tests). Label tests
include normalization, CLI operand/target parsing, legacy default/restart,
SQLite no-op total-change counts, rollback on injected cleanup failure, parallel
batch edits with metadata changes, deletion races and non-resurrection.

Built fresh debug executables using the same Cargo environment with
`cargo build --offline --locked --manifest-path /home/vfuchedzhy/proj/rsynapse/projd/Cargo.toml --bins`.

```sh
env PROJD_BIN=/home/vfuchedzhy/proj/rsynapse/steward/target/debug/rsynapse-projd \
  PROJ_BIN=/home/vfuchedzhy/proj/rsynapse/steward/target/debug/proj \
  python3 /home/vfuchedzhy/proj/rsynapse/projd/tests/labels.py

env PROJD_BIN=/home/vfuchedzhy/proj/rsynapse/steward/target/debug/rsynapse-projd \
  PROJ_BIN=/home/vfuchedzhy/proj/rsynapse/steward/target/debug/proj \
  python3 /home/vfuchedzhy/proj/rsynapse/projd/tests/integration.py
```

Both passed on temporary stores and private buses. Labels coverage includes
actual introspection signatures, read-only property declaration, ObjectManager,
add/remove property signals, no-op silence with an ordered signal barrier,
invalid-batch atomicity, FileNotFound, CLI targeting/defaults/JSON, independent
projects, concurrent clients/rename/icon/deletion, restart and fresh identity.
The existing integration suite also passed its project/worktree/icon/Git/goal
and metadata-only-removal checks. Its readiness probes print transient
ServiceUnknown while the private daemon is starting; the suite passes.

`git diff --check -- projd` passed. New Rust modules were formatted individually;
existing files received focused edits only.

## Handoff

No deployment or live-service tests were performed. The new private-bus test
requires Python PyGObject/Gio, which was available for verification.

Named semantic navigation was attempted first. The retained structured lspctl
failure on `definition --file projd/src/main.rs --symbol ProjectInfo` was
`server_selection_failed`, category `blocked`, reason/message
`No route matches the target file.`, delivery `not_sent`, retry `after_change`.
Source reads were used as fallback.

The service-created goal note path is
`/home/vfuchedzhy/.local/state/proj/goals/6d70c454fd634a81bfd003281e130afa.md`.
This repository note holds the implementation evidence to keep document edits
inside the assigned `projd/` scope. Parent can link it from the shared goal note.
