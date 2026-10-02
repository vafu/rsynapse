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
  completed focus intervals and immediately pushes on focus changes, lock,
  and shutdown. No heartbeat or timed flush. Uses the [`graphyne`](https://crates.io/crates/graphyne)
  crate (Apache-2.0) for the Graphite plaintext protocol.
- `namer`
  Assigns workspace display names into the `org.rsynapse.workspace.name`
  relation: preferred manual names win, then the project's cwd label,
  then `empty` for project-less workspaces. Old random defaults are
  replaced on selection. Automatic names are compared before writing;
  manual names are never overwritten. Click the bar's workspace title
  to edit and save a preferred name in locus.
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
  dashboard (project and app pies, workspace-switch total, switches/hour,
  project timeline, workspace-name timeline). Pies and switch totals use
  the selected dashboard time range; focus durations render in readable
  time units. Pies sum each series rather than taking its last sample.

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

Native `shell_source::wayland::input_idle()` splits focus attribution
after 30 seconds without keyboard/mouse input (`INPUT_IDLE_SECS` overrides
the threshold). It uses ext-idle-notify v2 input-only notifications, ignoring
idle inhibitors such as video playback. Timers keep running while idle;
durations are tagged `idle.true` or `idle.false`. Lock still stops focus attribution.
The threshold is a compositor-managed inactivity policy, not a timed flush:
only active/idle/locked transitions trigger reports. The initial notification
starts active; its first idle timeout begins when the source is created.

| metric | meaning |
|---|---|
| `rsynapse.focus.project.<name>.idle.<true,false>.seconds` | completed focus duration per locus project, split by idle state |
| `rsynapse.focus.workspace_name.<name>.idle.<true,false>.seconds` | same, keyed by workspace display name |
| `rsynapse.focus.app.<name>.idle.<true,false>.seconds` | focus duration per canonical app (`opencode`, `codex`, `neovim`, etc.), split by idle state |
| `rsynapse.focus.workspace.<id>.idle.<true,false>.seconds` | focus duration per niri workspace id, including project-less workspaces |
| `rsynapse.focus.window.<id>.idle.<true,false>.seconds` | completed window focus duration, split by idle state |
| `rsynapse.focus.output.<name>.idle.<true,false>.seconds` | focus duration per output, split by idle state |
| `rsynapse.switches.workspace` | workspace-switch increments emitted immediately |
| `rsynapse.switches.window` | window-switch increments emitted immediately |
| `rsynapse.switches.app` | app focus switches, including another window running the same app |
| `rsynapse.switches.project` | project focus switches on workspace changes |
| `rsynapse.switches.output` | output-focus switches |
| `rsynapse.activity.locked.seconds` | completed locked intervals |
| `rsynapse.activity.state` | transition-only gauge: 1 active, 0 idle, 2 locked |

Each dimension has its own timer. Window changes finish window/app intervals;
workspace changes finish workspace/project intervals. Initial selection and
unlock start timers without counting switches. Lock and graceful SIGTERM/SIGINT
shutdown close all intervals. Long intervals are recorded in full, with no cap.
While focus stays unchanged there are no writes; its duration appears when the
interval ends, timestamped at that event (not spread retroactively over time).
Input-idle transitions close the previous segment and immediately start a new
segment on the same focus with the new idle state, without counting a switch.
For example, idle -> active after 10 seconds on Codex reports 10 seconds in
`rsynapse.focus.app.codex.idle.true.seconds` and starts an `idle.false` segment.

Active and idle totals are derived, not emitted as redundant counters:
`sumSeries(rsynapse.focus.workspace.*.idle.true.seconds)` covers all idle
workspace time; the equivalent project query covers only project-assigned time.
Grafana's Activity filter selects All/Active/Idle for app/project/workspace
charts; All sums both idle states. Locked duration stays separate because no
workspace/app is focused while locked. Historical unsplit focus series are no
longer written or used by the dashboard.

Graphite stores a 10-second time grid. Every event is sent immediately; events
sharing a storage slot send the updated slot total so Graphite's replacement
semantics do not lose rapid switches. This requires no scheduling or waiting.
The provisioned storage aggregation rules sum sparse increments across
retention archives and use `last` for the activity-state gauge. Existing Whisper
files need their aggregation method updated separately when changing this policy.

Agent identity follows `org.rsynapse.window.agent-session` in locus and
subscribes to `AgentName` and `WindowId` on the linked AgentDBus session.
The live `WindowId` must match before the agent can override the app name.

`METRIC_PREFIX`, `CARBON_HOST`, `CARBON_PORT`, `INPUT_IDLE_SECS` env vars
override the defaults. If carbon is down, batches are dropped with a
stderr warning; the daemon keeps running and reconnects on the next
event.
