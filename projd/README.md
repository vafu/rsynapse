# Projd and proj

`projd` is a reusable, durable project-management D-Bus service. `proj` is its
CLI client. Agents use that same public client/API. Neither imports shell-source,
niri, locus, steward, AgentDBus, Graphite or Grafana.

## Objects and persistence

- Bus: `org.rsynapse.Proj`; manager: `/org/rsynapse/Proj`.
- `org.rsynapse.Proj.Manager1` manages projects, optional Git checkouts and goals.
- ObjectManager exposes Project1, Checkout1 and Goal1 objects.
- Each Git checkout/worktree has its own project and stable UUID identity.
  A non-Git directory project has no checkout. Worktrees have independent names
  and icons, even when their repositories share a Git common directory.
- Project1 exposes `Id`, `Name`, absolute `Cwd`, `Icon`, `IconOrigin` and `Checkout`.
  The checkout reference is a D-Bus object path (`o`); `/` means no checkout.
- Checkout1 exposes `Id`, `RootPath`, `Branch`, `GitStatus` and `Project`.
  The reverse project reference is derived from project ownership, not duplicated
  in the checkout record. Project/checkout paths use compact `p<ID>`/`c<ID>` segments.
- Explicit registration sets project CWD. Resolution and refresh leave stored
  CWD/name unchanged. There are no Context objects or relative-CWD descriptors.
- Goals own full titles, criteria, status, priority, kind and optional project.
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
proj update
proj goal list --json
proj goal add outcome1 --title 'Observable result' --project "$PWD" \
  --success 'Concrete criterion' --priority high
proj goal status outcome1 in-progress
proj goal status outcome1 completed
proj goal add habit1 --kind habit --title 'Personal budget' --success 'At most 30 minutes'
```

Use `--date YYYY-MM-DD` for another day or `goal list --all` for history.
`goal import FILE.json` is idempotent and preserves existing projd records.
Agents follow `skills/project-management/SKILL.md`; completion is explicit and
requires evidence, never an inference from an idle agent.

Project removal permanently deletes the project's registration and optional
checkout in one SQLite transaction. Goals/history remain stored. Project
folders, Git worktrees and files are untouched. There is no project trash bin,
tombstone or restore operation. ObjectManager removes the deleted objects and
the manager emits `ProjectRemoved` with their identities for integration cleanup.
Steward clears desktop associations while keeping saved workspace names.
`proj root`, `proj metadata`, `proj update` and automatic integrations resolve
existing registrations; they do not recreate a removed project. `proj add`
explicitly registers it again with fresh identities.
Goal CRUD retains its project path as historical domain data; it does not register
directories, change project CWD, or resurrect deleted project registrations.

Desktop commands from the old Bash helper (`set-current`, `refresh-current`,
`publish`, `clear`) are no longer project-management operations. To associate
the current workspace, use `steward bind-current PATH`, or click the shell's
initialize-project icon beside an unassociated workspace name. The old script
is kept only as `install/bin/legacy-proj` for migration reference.

## Consumers

`shell-source::proj` provides projects/checkouts/goals snapshots and
per-object name/icon/icon-origin/branch observables for UI and headless listeners. Steward owns
session associations and writes references into locusd. The shell composes
those references with project properties. The Grafana HTTP adapter calls this
API and stores no project-management data.

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
Context records are removed; goals are preserved unchanged. Steward remaps
desktop associations by checkout identity while retaining persistence flags,
adopts ID- and name-keyed workspace icon overrides, and updates old project
object-path references. Conflicting legacy icons are retained for review.
Update projd, proj, steward, dashboard-api and shell together when deploying this
internal protocol change.

## Build and verification

```sh
bash projd/build.sh --locked
python3 projd/tests/integration.py
python3 projd/tests/model_migration.py
python3 projd/tests/responsiveness.py
python3 steward/tests/model_migration.py
PROJD_BIN="$PWD/steward/target/release/projd" \
  cargo test --manifest-path shell/Cargo.toml -p shell-source \
  --test project_icons -- --ignored
PROJD_BIN="$PWD/steward/target/release/projd" \
  cargo test --manifest-path shell/Cargo.toml -p rsynapse-shell --lib \
  shell_autopicks_only_unset_project_icons -- --ignored
```

The integration test runs on a private bus without any desktop or other service.
It checks CLI/API parity, separate worktree projects, stored CWD, git notifications,
ObjectManager references, icons, goals and restart persistence. The icon source
test checks initial values, live property updates, late subscription replay,
association switching and manual/automatic precedence on a private bus.
See `install/migrate-projd.py` for backed-up imports from intentional legacy
workspace bindings; metadata-cache entries alone do not register projects.
