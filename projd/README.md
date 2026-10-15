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
- Goals/tasks own titles, descriptions, parent links, notes, status and optional
  project/date. Legacy daily outcomes and habits retain their criteria and priority.
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
proj goal add --title 'Ship the release' --description 'Publish a tested artifact' --active
proj goal add --parent GOAL_ID --title 'Build the artifact'
proj goal update GOAL_ID --title 'Ship v2' --description 'Updated context'
proj goal set release --title 'Ship the release'
proj goal start GOAL_ID
proj goal complete GOAL_ID
proj goal note GOAL_ID
proj goal note GOAL_ID --open
proj goal add outcome1 --kind outcome --title 'Observable result' --project "$PWD" \
  --success 'Concrete criterion' --priority high
proj goal status outcome1 in-progress
proj goal status outcome1 completed
proj goal add habit1 --kind habit --title 'Personal budget' --success 'At most 30 minutes'
```

Use `--date YYYY-MM-DD` for dated goals or `goal list --all` for the full catalog/history.
`goal import FILE.json` is idempotent and preserves existing projd records.
Agents follow `skills/project-management/SKILL.md`; completion is explicit and
requires evidence, never an inference from an idle agent.

## Persistent checklists and notes

`proj goal` (also `proj task`) defaults to persistent tasks. Creation generates a
stable UUID when no ID is given; named IDs support idempotent `goal set`. Tasks do
not expire at midnight. `goal list` shows the current registered project's tasks;
use `--project PATH` for another project or `--all` for the complete catalog.
`--date YYYY-MM-DD` is optional and keeps existing daily workflows available.
Use the full `date/ID` key when a historical ID occurs on several days.

`--parent ID` creates a subtask inheriting its parent's project. Subtasks can
nest. Status changes are explicit: `planned` (`todo` in the CLI), `in-progress`,
`completed`, or `deferred`. Completing a child does not complete its parent;
completing a parent does not change its children. Remove children before removing
their parent. `goal update` edits title/description without overwriting a concurrent
status change. `goal show ID` returns the full record as JSON.

Each project independently selects an active task. `goal active ID` selects an
open task; `goal active --clear` stops tracking it. `goal start ID` both marks it
in progress and selects it. Several tasks can be in progress while only one is
shown on that project's bar. Completing, deferring or removing the selected task
clears active selection. Selection persists across restart.
For multi-step work, keep the parent goal in progress and start the concrete
subgoal being worked on. The shell derives parent context from the hierarchy for
the tooltip/checklist, while the bar shows only the current subgoal title. No
duplicate parent-selection record is needed. If a parent remains selected, its
sole in-progress child supplies the title.

Creation also creates a Markdown note at `$XDG_STATE_HOME/proj/goals/<ID>.md`,
normally `~/.local/state/proj/goals/<ID>.md`. Dated goals use
`goals/dated/<date>/<ID>.md`. `PROJD_GOALS_DIR` overrides the notes directory;
when `PROJD_STORE_PATH` is overridden, notes default to `goals/` alongside that
test/custom store. Notes initially contain the title, description, Notes and Log
headings. Metadata/status edits never rewrite the file. Removing a task retains
its note. Projd's SQLite records are authoritative for checklist state; Markdown
is free-form agent/user context, not a second checklist database.

`goal note ID` prints its path for agents; `--open` runs `$EDITOR` (then `$VISUAL`,
then `vi`), preserving quoted command arguments. The shell launches this in
`xdg-terminal-exec`, `$TERMINAL`, or an available terminal such as Ghostty.

Every task/subtask is a `Goal1` object. It exposes `Key`, `Id`, `Title`,
`Description`, `Status`, `NotePath`, `ProjectPath`, and object-path references
`Project`, `Parent`, and `Children`. `Project1.ActiveGoal` references the tracked
task, with `/` meaning none. ObjectManager and PropertiesChanged provide live
state; `CreateGoal`, `UpdateGoal`, `EditGoal`, `SetGoalStatus`, `SetActiveGoal` and
`RemoveGoal` handle mutation. Hierarchy/active selection belong to projd, while
external workspace and agent links remain in locus.

Existing goal records retain IDs, dates, titles, criteria and statuses, gain
notes, and are backed up to `before-goal-checklists-*.sqlite3` before migration.
This extends the internal GoalInfo D-Bus signature; deploy the service and all
clients together.

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

## Goal history and work metrics

Goal metadata and lifecycle events are written in one SQLite transaction. Each
creation has an immutable instance UUID, distinct from its human-readable key.
Removing/recreating a named task preserves the old history without merging its
effort into the new task. Existing records receive an `observed` baseline with
unknown creation/first-start timestamps; timestamps are never inferred from dates
or Markdown modification times.

```sh
proj goal instances GOAL_ID --json
proj goal history GOAL_ID --json
proj goal timeline GOAL_ID --from today
proj goal stats GOAL_ID
proj goal stats GOAL_ID --direct --json
proj report --all --from 2026-10-01 --to 2026-11-01 --json
proj report --project /absolute/project --from today
proj report --project-id HISTORICAL_PROJECT_ID --json
```

History/timeline queries accept local dates, RFC3339 timestamps, or Unix
milliseconds. `--to` is exclusive and defaults to now. Named keys resolve to
their current instance, or the latest removed instance; pass an instance UUID
from `goal instances` to inspect an older lifetime. Stats include descendant work
by default; `--direct` excludes it. Reports group direct measured spans by
project and task instance, without adding parent totals a second time.

Stats distinguish input-active focus, idle focus, agent work, in-progress elapsed
time, creation-to-last-completion lead time and first-start-to-last-completion
cycle time. Lifecycle timestamps/completion counts cover the task lifetime;
effort and in-progress totals honor the requested range. Zero timestamps mean
unknown, and human output prints them as such. Focused/unlocked time is active
plus idle; agent time is a separate axis and may exceed wall time in parallel.
Work totals describe recorded coverage, not time reconstructed before collection.

The additive `org.rsynapse.Proj.Metrics1` interface lives at `/org/rsynapse/Proj`:
`ListGoalIdentities`, `ListGoalTracking`, `GoalHistory`, `WorkTimeline`, `GoalStats`,
`BeginGoalWork`, `GetWorkSession` and `EndGoalWork`. Work kind is a Rust enum
serialized as a D-Bus string (`active`, `idle`, `agent`). A producer owns its work
sessions through its unique D-Bus connection. Owner loss closes sessions;
graceful recorder shutdown closes them with measured monotonic durations. A
recorder crash marks surviving open records as interrupted/unknown-duration
coverage rather than counting unobserved downtime. Current open durations are
computed when queried, with no heartbeat or periodic database writes.

Steward submits `steward.focus` sessions from native focus/project/goal, idle and
lock events. It attributes a single effective leaf task, with immutable ancestor
IDs for rollups. Explicit selection resolves multiple in-progress siblings; a
selected parent's unique in-progress child is followed just as in the bar.
Projd stores generic work records and has no desktop source dependencies.

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
python3 projd/tests/checklist.py
python3 projd/tests/metrics.py
python3 projd/tests/model_migration.py
python3 projd/tests/responsiveness.py
python3 steward/tests/model_migration.py
PROJD_BIN="$PWD/steward/target/release/projd" \
  cargo test --manifest-path shell/Cargo.toml -p shell-source \
  --test project_icons -- --ignored
PROJD_BIN="$PWD/steward/target/release/projd" \
  cargo test --manifest-path shell/Cargo.toml -p shell-source \
  --test goals -- --ignored
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
