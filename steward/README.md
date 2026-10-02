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
  It also subscribes to all AgentDBus sessions for background agent-state
  durations, session counts, busy cycles, and response-to-read proxy latency.
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
  project and workspace-name state timelines). Pies and switch totals use
  the selected dashboard time range; focus durations render in readable
  time units. Pies sum each series rather than taking its last sample.
  Click the overview's Workspace breakdown pie to open the shared workspace
  detail page (UID `rsynapse-workspace`): app usage, idle percentage, total
  duration, active/idle breakdown, and that workspace's focus state. The link
  passes the clicked workspace, time range, and Activity filter. The detail
  page has a single-select context dropdown and a link back to the overview.
  Overview project/workspace focus charts each have one categorical lane
  containing the focused name or None, rather than one row per entity.

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
| `rsynapse.focus.workspace_name.<name>.app.<app>.idle.<true,false>.seconds` | joint app usage within a workspace context |
| `rsynapse.focus.project.<name>.state` | transition-only focus gauge: 0 not focused, 1 active focus, 2 idle focus |
| `rsynapse.focus.workspace_name.<name>.state` | same focus states per workspace display name |
| `rsynapse.focus.no_project.state` | 1 when there is no focused project, 0 when a project is focused |
| `rsynapse.focus.no_workspace_name.state` | 1 when there is no focused workspace, 0 otherwise |
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

## Workspace drill-down

The `workspace_name` query variable uses `rsynapse.focus.workspace_name.*`
to discover recorded contexts, including historical names and `empty`.
These are metric nodes, not an inventory of projects that have never produced
metrics. Workspaces with the same display name aggregate into the same context.
The overview has no repeated rows. Its workspace pie carries a single data
link using the clicked field display name; the destination uses the URL's
`var-workspace_name` to scope all workspace panels.

Joint workspace/app intervals end on workspace, window/app, workspace-name,
idle, lock, and shutdown boundaries. This separates Codex windows in different
contexts and starts new collection from deployment onward; old independent
app/workspace series cannot reconstruct that association retroactively.

App pies group the joint series by app and follow the global Activity filter.
Idle percentage is the ratio of **summed** idle workspace duration to **summed**
total workspace duration within the selected range, not an average of point
percentages. It and the Active / idle panel always include both idle states,
independently of the app Activity filter. Workspace-level totals include time
without an app window; no measured time produces No data for idle percentage.

The overview's focus timelines combine held-last presence gauges, convert
them to rows, keep only positive states, and retain Time + Metric as a single
string-valued lane. Separate no-focus gauges provide an explicit None segment
for locks, unassigned projects, or missing workspace focus. These gauges live
outside the workspace-name namespace so they cannot become workspace choices.
Idle transitions keep the same focused name; the Input activity lane shows
active/idle/locked status separately. Only real transitions are published.

## Dashboard colors

`grafana/focus-colors.json` is the shared name-to-color registry. Context
colors are applied to project/workspace pies, the single-lane focus timelines,
and the selected workspace's detail focus chart. App colors are shared by
the overview and detail app pies. None/not-focused stays gray.

Grafana's classic palette colors a field, not different string values within
one timeline field. Explicit value mappings color the timeline categories;
fixed-color series overrides apply the same assignments to the other panels.
Colors are independent of the selected time range and preserved as new names
are added. Refresh the provisioned mappings after new contexts/apps appear:

```sh
python3 steward/grafana/refresh-focus-colors.py
python3 steward/grafana/refresh-focus-colors.py --check
```

This is a manual configuration refresh, not a collector timer or a background
job. Grafana reloads the resulting dashboard files through its provisioning.

## Agent metrics

AgentDBus membership and live properties enter through `shell-source` D-Bus
observables. State intervals run even when a session's window is not focused
or the human is idle/locked. Events and shutdown close intervals immediately;
there is no polling or timed flush in this path. Durations are agent-seconds:
parallel sessions add together and can exceed elapsed wall time.

Each metric has two attribution axes (do not sum them together):

```text
rsynapse.agents.project.<project>.agent.<agent>.role.<root,subagent>.<metric>
rsynapse.agents.workspace_name.<context>.agent.<agent>.role.<root,subagent>.<metric>
```

Project attribution prefers the longest known project path containing the
session cwd, then the live window's workspace-project relation. Workspace
contexts use the live niri window and locus display name. Subagents inherit
their parent's attribution/window where missing. Unresolved cohorts are
explicitly `unassigned`; session IDs and paths do not become metric series.

| metric suffix | meaning |
|---|---|
| `state.<state>.seconds` | completed thinking/tool-use/idle/compacting/other state intervals |
| `sessions.live.state` | current exported session-object count, updated on change |
| `sessions.busy.state` | current thinking/tool-use/compacting session count |
| `sessions.started.count` | sessions first appearing after the initial roster baseline |
| `responses.completed.count` | observed busy -> idle transitions (completion proxy) |
| `responses.read.count` | idle responses acknowledged via the read proxy |
| `responses.waiting.state` | current unread-response count |
| `responses.unread_cancelled.count` | response superseded by work/session close before acknowledgment |
| `response_latency.seconds_sum` / `response_latency.count` | wall-clock reaction-delay sum and sample count |
| `response_latency.available_seconds_sum` | reaction-delay sum excluding human input-idle/locked periods |
| `work_cycles.seconds_sum` / `work_cycles.count` | fully observed idle -> busy -> idle cycle durations and counts |

Read means the associated window is focused, unlocked, and input-active. This
mirrors the shell's seen badge as a focus-based proxy, not proof of reading.
Locus's active session link disambiguates sessions sharing a terminal window.
Already-visible completions yield zero delay. Startup-idle sessions are never
invented as completions; cycles already running at startup have no full-cycle
sample. A collector restart baselines existing sessions rather than counting
them as newly started. Root sessions alone get human reaction metrics; child
state times/counts remain separately selectable.

The overview shows root agent work and exported sessions by project. The
workspace detail page adds state durations, new/live/busy sessions, completions,
unread responses, mean cycle time, and both reaction-delay means. `Agent scope`
selects roots/subagents/All independently of the human Activity filter. Means
are ratios of summed durations to summed counts, not averages of slot means.

AgentDBus currently exposes no input/output/cache token counters. `ContextPct`
is context occupancy, not consumed tokens; `CostUsd` is currently zero for the
observed producers, so neither is emitted as fabricated token/cost usage.

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
