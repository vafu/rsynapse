# Projd and proj

`projd` is a reusable, durable project-management D-Bus service. `proj` is its
CLI client. Agents use that same public client/API. Neither imports shell-source,
niri, locus, steward, AgentDBus, Graphite or Grafana.

## Objects and persistence

- Bus: `org.rsynapse.Proj`; manager: `/org/rsynapse/Proj`.
- `org.rsynapse.Proj.Manager1` manages projects/contexts and goals.
- ObjectManager exposes Project1, Checkout1, Context1 and Goal1 objects.
- Projects have stable UUID identities; one project can own several worktrees.
- Checkouts own branch and git-status properties. CWD contexts remain distinct,
  so another window/agent cannot overwrite one global project CWD or branch.
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

Project removal permanently deletes the project's registration, checkouts and
CWD contexts in one SQLite transaction. Goals/history remain stored. Project
folders, Git worktrees and files are untouched. There is no project trash bin,
tombstone or restore operation. ObjectManager removes the deleted objects and
the manager emits `ProjectRemoved` with their identities for integration cleanup.
Steward clears desktop associations while keeping saved workspace names.
`proj root`, `proj metadata`, `proj update` and automatic integrations resolve
existing registrations; they do not recreate a removed project. `proj add`
explicitly registers it again with fresh identities.

Desktop commands from the old Bash helper (`set-current`, `refresh-current`,
`publish`, `clear`) are no longer project-management operations. To associate
the current workspace, use `steward bind-current PATH`, or click the shell's
initialize-project icon beside an unassociated workspace name. The old script
is kept only as `install/bin/legacy-proj` for migration reference.

## Consumers

`shell-source::proj` provides projects/checkouts/contexts/goals snapshots and
per-object name/branch observables for UI and headless listeners. Steward owns
session associations and writes references into locusd. The shell composes
those references with project properties. The Grafana HTTP adapter calls this
API and stores no project-management data.

## Build and verification

```sh
bash projd/build.sh --locked
python3 projd/tests/integration.py
python3 projd/tests/responsiveness.py
```

The integration test runs on a private bus without any desktop or other service.
It checks CLI/API parity, worktree identity, CWD separation, git notifications,
ObjectManager, goals and restart persistence. See `install/migrate-projd.py`
for backed-up imports from legacy locus metadata.
