# Shared observability deployment

Graphite and Grafana are independent services. Steward writes the existing
Carbon plaintext protocol on 127.0.0.1:2003; there is no replacement metrics API.
Grafana's goal metrics and workday settings panels are clients via dashboard-api.

```sh
docker compose -f misc/observability/docker-compose.yml up -d
```

The Compose project remains `metrics`, preserving its named storage volumes.
Generic dashboards and Graphite retention aggregation live here, outside steward
and projd. Projd has no dependency on this deployment. Tracked dashboards contain
no discovered project, workspace, or application inventory; Grafana's normal
palette and query-backed selectors work on a new installation.

## Optional local color assignments

To give project/workspace names consistent colors across panels, render local
dashboards from Graphite discovery:

```sh
python3 observability/grafana/refresh-focus-colors.py
RSYNAPSE_GRAFANA_DASHBOARDS=./grafana/generated/dashboards \
  docker compose -f misc/observability/docker-compose.yml up -d grafana
```

The renderer writes all dashboards under the ignored
`observability/grafana/generated/dashboards/` directory. Stable color assignments
live at `$XDG_STATE_HOME/rsynapse/grafana/focus-colors.json` (default
`~/.local/state/rsynapse/grafana/focus-colors.json`). `--output-dir`, `--registry`,
and `--graphite-url` override these locations. `--check` verifies the rendered
files without writing. Runtime discovery never updates the tracked dashboards.

The offline `--generic` mode removes discovered mappings/overrides and needs no
Graphite or registry. To regenerate/check the committed baseline:

```sh
python3 observability/grafana/refresh-focus-colors.py --generic \
  --output-dir observability/grafana/dashboards
python3 observability/grafana/refresh-focus-colors.py --generic --check \
  --output-dir observability/grafana/dashboards
python3 observability/grafana/test_refresh_focus_colors.py
```

The **Rsynapse goal work metrics** dashboard is provisioned at
http://localhost:3000/d/rsynapse-goals/rsynapse-goals. Its local panel reads
goal-instance history and work spans through dashboard-api, with Grafana range
selection and native refresh. Focus overview and productivity dashboards link to
it. Goal data remains authoritative in projd SQLite; Graphite continues serving
the existing aggregate metric series.

The productivity dashboard includes **Workday target settings**, a standalone
span/unlocked/active preference editor. Goal/task management lives in `proj goal`
and the shell checklist. The former daily-goal Grafana editor and its HTTP CRUD
routes have been retired; existing goal records and metrics history remain in projd.
