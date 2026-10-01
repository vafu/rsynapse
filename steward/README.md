# Rsynapse steward

Headless session steward: focus-time metrics, workspace naming, and
per-project git status. Small single-purpose components (metrics, namer,
git-status) share one bus connection through a thin locus client; reads
go through `shell-source` observables, writes through the client.

Local-only focus-time metrics: which project, app, output, and workspace
holds focus, plus workspace-switch counts. Everything stays on this
machine: the emitter pushes Graphite plaintext to a local carbon daemon,
and Grafana visualizes it. No accounts, no cloud, no window titles ever
leaving the process (only app-ids and project names become metric paths).

## Components (`src/`)

- `metrics`
  Folds niri focus streams and locus workspace→project relations into
  per-bucket seconds and pushes batches to carbon every `FLUSH_SECS`
  (default 10s). Uses the [`graphyne`](https://crates.io/crates/graphyne)
  crate (Apache-2.0) for the Graphite plaintext protocol.
- `namer`
  Assigns workspace display names into the `org.rsynapse.workspace.name`
  relation: manual names win, then project display names, then stable
  random words for focused project-less workspaces.
- `git_status`
  Polls every mapped project path and publishes changed snapshots as
  `org.rsynapse.project.git-status` relations, so UI processes never
  shell out to git.
- `locus.rs`
  The only zbus-aware file: a thin write client plus `records()`
  observables over locus relations. Components compose observables and
  write through the client.
- `docker-compose.yml`
  `graphiteapp/graphite-statsd` (MIT image; carbon/whisper/graphite-web
  are Apache-2.0) plus `grafana/grafana-oss` (AGPL-3.0). Ports bind
  `127.0.0.1` only: carbon `2003`, graphite-web `8080`, Grafana `3000`.
- `grafana/`
  Provisioned Graphite datasource and the starter "Rsynapse focus"
  dashboard (project stack, 24h app pie, switches today, switches/hour,
  workspace stack, session stack, workspace-name stack).

## Registry note

This crate resolves dependencies from the **public crates.io** registry,
not the internal mirror from the user-wide cargo config. Two mechanisms
enforce that:

- `build.sh` runs cargo from a neutral CWD (`/tmp`, outside `$HOME` so
  cargo's config walk-up never finds the mirror config) with an isolated
  `CARGO_HOME` (`steward/.cargo-home`) and a repo-local target dir.
- `steward/.cargo/config.toml` points crates-io at a nonexistent source so
  a direct `cargo` run here fails loudly instead of silently using the
  mirror. Always use `build.sh`.

## Run it

```sh
# 1. storage + viz
docker compose -f steward/docker-compose.yml up -d

# 2. emitter (first run fetches crates.io deps; subsequent builds reuse steward/target)
./steward/build.sh

# 3. install + start the user service (unit is installed by install/local.sh)
install -m755 steward/target/release/rsynapse-steward ~/.local/bin/rsynapse-steward
systemctl --user enable --now rsynapse-steward.service
```

Grafana: http://localhost:3000 (default `admin`/`admin`).
Graphite-web: http://localhost:8080.

Verify data is landing:

```sh
curl -s 'http://localhost:8080/render?target=rsynapse.switches.workspace&format=json&from=-5min' | head -c 300
```

## Metrics

Focus attribution is gated by `shell_source::session::locked()` (logind
`LockedHint`). Locking ends the current focus interval immediately; no
app, project, workspace, or output time accrues while locked. Unlocking
resumes from the latest niri focus, and neither transition counts as a
workspace switch. Lockers must update logind's `LockedHint` for this to work.

| metric | meaning |
|---|---|
| `rsynapse.focus.project.<name>.seconds` | focus seconds per locus project (10s heartbeat) |
| `rsynapse.focus.workspace_name.<name>.seconds` | same, keyed by display name: merges workspaces sharing a project and survives workspace-id churn |
| `rsynapse.focus.app.<name>.seconds` | focus seconds per canonical app: hook app-instance name (`codex`, `neovim`, …) else canonicalized AppId |
| `rsynapse.focus.workspace.<id>.seconds` | focus seconds per niri workspace id |
| `rsynapse.focus.output.<name>.seconds` | focus seconds per output |
| `rsynapse.switches.workspace` | workspace-focus changes per flush |

`METRIC_PREFIX`, `CARBON_HOST`, `CARBON_PORT`, `FLUSH_SECS` env vars
override the defaults. If carbon is down, batches are dropped with a
stderr warning; the daemon keeps running and reconnects on the next
flush.
