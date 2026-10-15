# Local dashboard API and Grafana panels

This project is a small loopback HTTP/SSE client adapter over projd and session
integration, plus read-only goal metrics and workday target settings panels.
Projd owns all project and goal state; neither its daemon nor CLI knows about Grafana.
It also builds `rsynapse-workday-panel`, the overview's recorded-start/range
control. On load with no explicit range, it selects Started today → now.
Explicit/manual ranges take precedence; automatic ranges are tagged in the URL
so reloading them can pick up a new day's start. It updates once per load, not
on each data refresh. The panel's auto-range option can disable that default.
`rsynapse-goal-metrics-panel` adds read-only work timelines, effort and lifecycle
views to the **Rsynapse goal work metrics** dashboard.

```text
Grafana goal metrics <- HTTP/SSE adapter <- projd Metrics1 <- SQLite
Grafana workday settings <-> HTTP/SSE adapter <-> locusd preferences

Steward metrics -> standard Carbon -> independent Graphite/Grafana deployment
```

The adapter has no database, project-management logic or automatic completion.
Projd's SQLite store owns full goal records and metrics history. Locus stores
cross-service associations and workday preferences. `install/migrate-projd.py`
backs up and imports saved legacy goals/projects before removing full metadata
mirrors. Manage goals with `proj goal` and the shell checklist.

## Use

### Goal work metrics

Open http://localhost:3000/d/rsynapse-goals/rsynapse-goals. The panel follows the
Grafana time picker and dashboard refresh, plus live D-Bus/SSE changes. Select a
project and optional goal/task instance; old removed instances remain selectable.
The views include:

- Timestamped human active/idle and separate agent lanes by project and task.
- Focus effort by project and totals calculated from raw intervals once.
- Per-goal effort, recorded in-progress duration, creation/start/completion dates,
  lead/cycle times and explicit unknown/partial coverage.
- Lifecycle/status/selection/removal events in the selected range.

Parent rows include subgoal effort when enabled; do not sum those rows to get an
overall total. Summary totals use each interval only once. Date/effort filtering
does not change lifetime completion dates. Open spans are calculated when queried;
Grafana refresh is visualization-only, not a recording heartbeat. Unknown legacy
timestamps are not converted to zero-duration completion times. Input-active time
is an activity estimate; idle focus remains separate. Agent sessions are supported
by the store/API, while automatic collection currently records human focus.

The browser reads the loopback API, which reads projd Metrics1. There is no new
Grafana datasource database or goal-title series in Graphite.

### Workday target settings

Open **Workday target settings** in the Rsynapse productivity dashboard.
The `rsynapse-workday-targets-panel` uses **Edit workday targets** to configure elapsed span,
unlocked and active hours. Defaults are 8 / 6 / 4 hours. Preferences are stored
in locus and apply reactively to today/future days; earlier daily log rows keep
their recorded targets. Active hours cannot exceed unlocked hours.
The panel follows workday-preference SSE notifications without polling, preserving
unsaved edits during live updates. Errors leave the draft available for correction.

The old daily-goal editor, calendar/CRUD routes and session-link picker have been
removed. Existing dated goals, notes and metrics history remain in projd. Use
`proj goal` to create/edit tasks and the shell checklist to complete, select or
open their notes; Grafana focuses on recorded metrics and workday settings.

## Build and local install

From the repository root:

```sh
bash install/dashboard-api.sh
```

This builds Rust against public crates.io using the steward's isolated Cargo
cache/target directory, builds the panel from pinned npm dependencies, installs
the API binary and user service, and recreates Grafana with the panel mounted.
It preserves the Grafana storage volume and enables only the three local panels'
unsigned-plugin IDs. `PREFIX` overrides the default `~/.local` binary prefix.
The GTK shell installer remains separate.

API defaults:

- Address: `127.0.0.1:8770` (never an all-interfaces bind).
- `RSYNAPSE_DASHBOARD_PORT` overrides the loopback port.
- `RSYNAPSE_DASHBOARD_ORIGINS` is a comma-separated CORS origin list; defaults
  to `http://localhost:3000,http://127.0.0.1:3000`.
- Each HTTP-backed panel's API URL option overrides its URL when needed.

## HTTP contract

| Method and path | Meaning |
|---|---|
| `GET /api/goal-metrics` | Read-only composite Metrics1 projection for Grafana |
| `GET/PUT /api/workday-targets` | Read/save daily span, unlocked and active hour preferences |
| `GET /api/events` | Domain/work/target change notifications via SSE |
| `GET /health` | Adapter readiness |

`/api/goal-metrics` accepts `from_ms`, exclusive `to_ms` (0/omitted means now),
`project_id`, immutable `instance_id`, and `include_subgoals` (default true).
It returns the echoed range, project/task-instance catalog, scoped work intervals,
stats and lifecycle events. Effort fields use one filtered interval snapshot,
including historical project attribution after task moves. SSE emits
`goal-metrics-changed` on Metrics1 WorkChanged, `goals-changed` on relevant
domain changes, and `workday-targets-changed` on preference changes. Neither
metrics nor events writes domain data.

The shared Rust record schema is in `../projd/model/`; steward depends on that
schema for reading associations. Projd performs durable domain writes and emits
property changes. Locus performs explicit-persistence preference writes. This adapter is a UI
client, never a second project-management service.

## Verification

```sh
bash dashboard-api/build.sh --locked
python3 dashboard-api/tests/integration.py
python3 dashboard-api/tests/metrics.py
npm --prefix dashboard-api/grafana-plugin run build:test
npm --prefix dashboard-api/grafana-plugin test
```

The integration test creates a private D-Bus and temporary locus store. It checks
workspace associations, retired routes, untouched goal records, workday preference
validation/SSE/CORS/restart persistence, and the read-only metrics projection.
Browser tests use installed Chrome (`CHROME_BIN` overrides its path) for metrics,
workday settings, error/draft preservation and range controls against fixture APIs.
