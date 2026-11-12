# Local dashboard API and Grafana panels

This project is a small loopback HTTP/SSE client adapter over projd and session
integration. It serves workday target settings; task/goal metrics are no longer
owned or exposed by projd. Neither projd nor its CLI knows about Grafana.
It also builds `rsynapse-workday-panel`, the overview's recorded-start/range
control. On load with no explicit range, it selects Started today → now.
Explicit/manual ranges take precedence; automatic ranges are tagged in the URL
so reloading them can pick up a new day's start. It updates once per load, not
on each data refresh. The panel's auto-range option can disable that default.

```text
Grafana workday settings <-> HTTP/SSE adapter <-> locusd preferences

Steward metrics -> standard Carbon -> independent Graphite/Grafana deployment
```

The adapter has no database, project-management logic or automatic completion.
Projd owns project/checkout metadata. Historical goal rows may remain in SQLite
but are no longer served. Tasks live as remarked notes. Locus stores
cross-service associations and workday preferences.

## Use

### Workday target settings

Open **Workday target settings** in the Rsynapse productivity dashboard.
The `rsynapse-workday-targets-panel` uses **Edit workday targets** to configure elapsed span,
unlocked and active hours. Defaults are 8 / 6 / 4 hours. Preferences are stored
in locus and apply reactively to today/future days; earlier daily log rows keep
their recorded targets. Active hours cannot exceed unlocked hours.
The panel follows workday-preference SSE notifications without polling, preserving
unsaved edits during live updates. Errors leave the draft available for correction.

The old daily-goal editor, calendar/CRUD routes, session-link picker and goal
metrics panel have been removed. Historical rows/notes remain on disk but are
not served. Manage tasks in remarked; Grafana only edits workday settings.

## Build and local install

From the repository root:

```sh
bash install/dashboard-api.sh
```

This builds Rust with the caller's Cargo configuration, builds the panel from
pinned npm dependencies using the caller's npm registry configuration, installs
the API binary and user service, and recreates Grafana with the panel mounted.
It preserves the Grafana storage volume and enables only the three local panels'
unsigned-plugin IDs. `PREFIX` overrides the default `~/.local` binary prefix.
The GTK shell installer remains separate.
Standalone builds use `misc/dashboard-api/target` unless `CARGO_TARGET_DIR` is set;
projd and Steward use their own target directories by default.

API defaults:

- Address: `127.0.0.1:8770` (never an all-interfaces bind).
- `RSYNAPSE_DASHBOARD_PORT` overrides the loopback port.
- `RSYNAPSE_DASHBOARD_ORIGINS` is a comma-separated CORS origin list; defaults
  to `http://localhost:3000,http://127.0.0.1:3000`.
- Each HTTP-backed panel's API URL option overrides its URL when needed.

## HTTP contract

| Method and path | Meaning |
|---|---|
| `GET/PUT /api/workday-targets` | Read/save daily span, unlocked and active hour preferences |
| `GET /api/events` | Domain/work/target change notifications via SSE |
| `GET /health` | Adapter readiness |

SSE emits `workday-targets-changed` on preference changes.

The shared Rust record schema is in `../projd/model/`; steward depends on that
schema for reading associations. Projd performs durable domain writes and emits
property changes. Locus performs explicit-persistence preference writes. This adapter is a UI
client, never a second project-management service.

## Verification

```sh
bash misc/dashboard-api/build.sh --locked
python3 dashboard-api/tests/integration.py
npm --prefix dashboard-api/grafana-plugin run build:test
npm --prefix dashboard-api/grafana-plugin test
```

The integration test creates a private D-Bus and temporary locus store. It checks
workspace associations, retired routes, untouched goal records, workday preference
validation/SSE/CORS/restart persistence, and the read-only metrics projection.
Browser tests use installed Chrome (`CHROME_BIN` overrides its path) for workday
settings, error/draft preservation and range controls against fixture APIs.
