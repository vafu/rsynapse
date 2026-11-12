# Projd and proj

`projd` is a reusable, durable project-management D-Bus service. `proj` is its
CLI client. Agents use that same public client/API. Neither imports data-sources,
niri, locus, steward, AgentDBus, Graphite or Grafana.

> **Tasks are owned by remarked.** Projd owns project/checkout metadata and
> optional labels/icons only. The former `Goal1`/`Metrics1` surfaces and `proj
> goal` CLI are gone. Tasks are `kind/task` notes, and active selection is the
> `active/<project>` tag enforced by remarked. Until the script system exists,
> use `scripts/remarked-active-task.py` to move active selection; it delegates to
> remarked and never writes task state into projd.

## Objects and persistence

- Bus: `org.rsynapse.Proj`; manager: `/org/rsynapse/Proj`.
- `org.rsynapse.Proj.Manager1` manages projects and optional Git checkouts.
- ObjectManager exposes Project1 and Checkout1 objects.
- Each Git checkout/worktree has its own project and stable UUID identity.
  A non-Git directory project has no checkout. Worktrees have independent names
  and icons, even when their repositories share a Git common directory.
- Project1 exposes `Id`, `Name`, absolute `Cwd`, `Icon`, `IconOrigin` and `Checkout`.
  The checkout reference is a D-Bus object path (`o`); `/` means no checkout.
- Project1 also exposes read-only `Labels` (`as`), a generic project-owned set.
  Labels have no workspace, shell, or GitHub semantics in projd.
- Checkout1 exposes `Id`, `RootPath`, `Branch`, `GitStatus` and `Project`.
  The reverse project reference is derived from project ownership, not duplicated
  in the checkout record. Project/checkout paths use compact `p<ID>`/`c<ID>` segments.
- Explicit registration sets project CWD. Resolution and refresh leave stored
  CWD/name unchanged. There are no Context objects or relative-CWD descriptors.
- Changed properties emit PropertiesChanged; root `Changed(kind,id)` supports
  typed collection clients. A non-Git directory can also be a project.

The SQLite store defaults to
`$XDG_STATE_HOME/rsynapse/projd/projects.sqlite3` (or `~/.local/state/...`).
`PROJD_STORE_PATH` overrides it. WAL and full synchronous writes are enabled.
The database belongs to projd, not locus or an HTTP adapter.

Git control files (HEAD/index/refs) trigger reactive refreshes. Worktree edits
are also refreshed by `proj update`, git hooks, and steward session triggers.
This first implementation does not recursively watch every file of giant
worktrees. New domain modules/checklists can extend the object model separately.
Directory registration resolves identities and branch without waiting for a
full Git status scan. Scans run per checkout outside the domain mutation lock;
watcher events retain at most one pending rescan per checkout. Worktree control
files do not trigger scans of the primary checkout, and lock files are ignored.

## CLI

```sh
proj add --name 'My project'
proj project list --json
proj remove PROJECT_ID
proj checkout list --json
proj root
proj metadata --json
proj project labels get --json
proj project labels add bug 'team name' --project /absolute/project --json
proj project labels remove bug --project-id PROJECT_ID
proj update
```

Tasks are managed by remarked tasks/notes, not `proj goal`. For the temporary
active-task bridge:

```sh
scripts/remarked-active-task.py tasks/new.md
scripts/remarked-active-task.py --from tasks/old.md tasks/new.md
scripts/remarked-active-task.py --project PROJECT_ID --clear
```

## Project labels

`proj project labels get|add|remove` defaults to the existing project resolved from
the current directory. `--project PATH` and `--project-id ID` are mutually exclusive;
none of these commands registers a directory. Add/remove accept one or more label
operands and return the resulting set, one label per line or a JSON array with
`--json`. Quote labels containing spaces; use `--` before operands beginning with
`-`. `proj metadata --json` includes `labels` as a JSON array.

Manager1 adds `AddProjectLabels(project_id: s, labels: as) -> as` and
`RemoveProjectLabels(id: s, labels: as) -> as`. Both normalize and validate the
whole batch before mutation: leading/trailing Unicode whitespace is trimmed,
empty strings and control characters are rejected (`InvalidArgs`), and spelling,
case and internal spaces are preserved. Deduplication and sorting use exact,
case-sensitive strings, without locale rules or Unicode normalization. Unknown
project IDs return `FileNotFound`, just as rename does. Empty API batches are
valid no-ops; the CLI requires operands for add/remove.

Labels are stored as `project-labels` records keyed by project UUID in the same
SQLite `records` table. Existing projects default to `[]` without a schema
backfill. Project deletion removes its labels transactionally; re-registration
gets a new identity with no labels. Atomic server-side batch edits share the
domain mutation lock with rename/icon/removal, preserve unrelated edits, and
never recreate missing projects. A changed batch commits before emitting
`Project1.Labels` PropertiesChanged and root `Changed("project", id)`. No-op label
methods write no records and emit neither signal. ObjectManager includes labels
in initial object state, including after restart. There is no label setter.

`ProjectInfo` and every existing D-Bus struct/method signature remain unchanged;
clients read labels from the additive Project1 property. Labels are not copied
into project registration, rename, icon, or list DTOs.

## Historical records

Earlier releases stored goal records and work metrics in projd's SQLite database.
They are no longer read, published, mutated, or served after this cleanup. The
raw rows are intentionally left in place for a later explicit migration; projd
does not delete user data on upgrade. Historical goal Markdown files under
`$XDG_STATE_HOME/proj/goals/` are likewise untouched.

## Consumers

`data-sources::proj` provides project/checkout snapshots and per-object
name/icon/icon-origin/branch observables, plus a remarked active-task adapter for
UI consumers. Steward owns session associations and writes references into
locusd. The shell composes those references with project properties. The
Grafana HTTP adapter stores no project-management data.

## Icons and migration

The shell subscribes to the associated project's `Icon` property, reads its
initial value and follows `PropertiesChanged`. Switching association replaces
that subscription. An unset icon is chosen by shell heuristics, then persisted
through `SetProjectIconIfUnset`. The conditional service write cannot replace a
manual selection. `SetProjectIcon` stores a manual choice; `ClearProjectIcon`
unsets it so the shell can choose again. `IconOrigin` is `manual`, `automatic`,
or empty. Icons persist with projects rather than workspace associations.

On first opening the old grouped-project schema, projd creates a
`before-model-v2-<timestamp>.sqlite3` backup and migrates in one transaction.
Each legacy checkout becomes a project; the primary keeps its original project
ID and custom name, and other worktrees start with their directory names.
Context records are removed. Historical goal records and notes are preserved but no longer read or served. Steward remaps
desktop associations by checkout identity while retaining persistence flags,
adopts ID- and name-keyed workspace icon overrides, and updates old project
object-path references. Conflicting legacy icons are retained for review.
Update projd, proj, steward, dashboard-api and shell together when deploying this
internal protocol change.

## Build and verification

The service executable and unit are `rsynapse-projd` and
`rsynapse-projd.service`; installation retains `projd` command/unit aliases.
`proj` remains the client command and project persistence/API ownership is unchanged.

```sh
bash dbus/projd/build.sh --locked
python3 projd/tests/integration.py
python3 projd/tests/labels.py
python3 projd/tests/model_migration.py
python3 projd/tests/responsiveness.py
python3 steward/tests/model_migration.py
PROJD_BIN="$PWD/dbus/projd/target/release/rsynapse-projd" \
  cargo test -p data-sources \
  --test project_icons -- --ignored
PROJD_BIN="$PWD/dbus/projd/target/release/rsynapse-projd" \
  cargo test --manifest-path apps/shell/Cargo.toml -p rsynapse-shell --lib \
  shell_autopicks_only_unset_project_icons -- --ignored
```

Builds use the caller's normal Cargo configuration and write to `dbus/projd/target`
unless `CARGO_TARGET_DIR` is set. The Python checks use release artifacts from
that same location; tests accepting `PROJD_BIN`/`PROJ_BIN` can target other builds.

The integration test runs on a private bus without any desktop or other service.
It checks CLI/API parity, separate worktree projects, stored CWD, git notifications,
ObjectManager references, icons and restart persistence.
The label test uses its own temporary SQLite store and private bus; it verifies
old-store defaults, unchanged wire signatures, property changes/no-op silence,
atomic batch validation, CLI targeting/JSON, races with rename/icon/removal,
project independence and restart persistence. It requires Python PyGObject (Gio).
The icon source test checks initial values, live property updates, late subscription replay,
association switching and manual/automatic precedence on a private bus.
See `install/migrate-projd.py` for backed-up imports from intentional legacy
workspace bindings; metadata-cache entries alone do not register projects.
