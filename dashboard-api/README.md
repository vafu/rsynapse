# Local dashboard API and Grafana goals panel

This project is a small loopback HTTP/SSE client adapter over projd and session
integration, plus the `rsynapse-goals-panel` Grafana UI. Projd owns all project
and goal state; neither its daemon nor CLI knows about Grafana.
It also builds `rsynapse-workday-panel`, the overview's recorded-start/range
control. On load with no explicit range, it selects Started today → now.
Explicit/manual ranges take precedence; automatic ranges are tagged in the URL
so reloading them can pick up a new day's start. It updates once per load, not
on each data refresh. The panel's auto-range option can disable that default.

```text
Grafana goal editor <-> HTTP/SSE adapter <-> projd D-Bus <-> SQLite
                            |
                    steward association API <-> locusd

Steward metrics -> standard Carbon -> independent Graphite/Grafana deployment
```

The adapter has no database, project-management logic or automatic completion.
Projd's SQLite store owns full goal records; locus contains only day/project/
agent references. `install/migrate-projd.py` backs up and imports saved legacy
goals/projects before removing full metadata mirrors. Agent links are requested
through steward; the adapter reads associations through locus.

## Use

Open the **Daily goals — plan, edit and review** panel in the Rsynapse
productivity dashboard. Choose a day and use **New goal**, **Edit**, the status
dropdown, or **Delete**. Full titles and success criteria appear in the panel.
Outcomes require an absolute project path; habits can be project-independent.
The **Sessions** button on an outcome lets you add/remove stable agent-session
associations. Matching open-goal project paths are associated by the collector
without an explicit session link. Habits never grant work credit.

The editor subscribes to SSE notifications sourced from projd's D-Bus changes
and locus association changes. It refreshes on date selection and Refresh without
a polling timer. Status changes are explicit; there is no weighted-completion
formula in steward. Goal titles, criteria and priority never enter Graphite.

Use **Edit workday targets** in the same panel to configure elapsed span,
unlocked and active hours. Defaults are 8 / 6 / 4 hours. Preferences are stored
in locus and apply reactively to today/future days; earlier daily log rows keep
their recorded targets. Active hours cannot exceed unlocked hours.

## Build and local install

From the repository root:

```sh
bash install/dashboard-api.sh
```

This builds Rust against public crates.io using the steward's isolated Cargo
cache/target directory, builds the panel from pinned npm dependencies, installs
the API binary and user service, and recreates Grafana with the panel mounted.
It preserves the Grafana storage volume and enables only the two local panels'
unsigned-plugin IDs. `PREFIX` overrides the default `~/.local` binary prefix.
The GTK shell installer remains separate.

API defaults:

- Address: `127.0.0.1:8770` (never an all-interfaces bind).
- `RSYNAPSE_DASHBOARD_PORT` overrides the loopback port.
- `RSYNAPSE_DASHBOARD_ORIGINS` is a comma-separated CORS origin list; defaults
  to `http://localhost:3000,http://127.0.0.1:3000`.
- The panel's **Local goal API** option overrides its URL when needed.

## HTTP contract

| Method and path | Meaning |
|---|---|
| `GET /api/goals?date=YYYY-MM-DD` | Full goal records for a day |
| `GET /api/goal-days` | Current local day and dates with stored goals |
| `GET/PUT /api/workday-targets` | Read/save daily span, unlocked and active hour preferences |
| `POST /api/goals` | Create a goal; duplicate IDs for the day return 409 |
| `PUT /api/goals/{date}/{id}` | Replace editable record fields with a full record |
| `PATCH /api/goals/{date}/{id}` | Update only supplied editable fields |
| `PATCH /api/goals/{date}/{id}/status` | Explicit status update |
| `DELETE /api/goals/{date}/{id}` | Delete goal and session associations |
| `GET/POST/DELETE /api/goals/{date}/{id}/links` | Read/add/remove agent/session links |
| `GET /api/projects` | Registered project/checkout paths from projd |
| `GET /api/sessions` | Live root AgentDBus sessions for link selection |
| `GET /api/events` | Goal/link change notifications via SSE |

The shared Rust record schema is in `../projd/model/`; steward depends on that
schema for reading associations. IDs and dates are immutable during editing.
The UI sends field diffs so editing a title does not roll back an independently
updated status. Projd performs durable domain writes and emits property changes.
Locus performs explicit-persistence association writes. This adapter is a UI
client, never a second project-management service.

## Verification

```sh
bash dashboard-api/build.sh --locked
python3 dashboard-api/tests/integration.py
npm --prefix dashboard-api/grafana-plugin run build:test
npm --prefix dashboard-api/grafana-plugin test
```

The integration test creates a private D-Bus and temporary locus store. It checks
CRUD, status/criteria preservation, duplicate races, SSE, links, CORS and restart
persistence. The browser test uses installed Chrome (`CHROME_BIN` overrides its
path) to exercise the actual React editor against a disposable fixture API.
