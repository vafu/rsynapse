# App Widget Performance Review

Review date: 2026-07-04

Scope: app-level widgets and widget-local sources under `app/src/widgets`, with
some app support paths that are called from widgets. I read `AGENTS.md`,
`PROJECT.md`, `PLAN.md`, `SOURCE_API.md`, and `app/src/widgets/AGENTS.md`
before reviewing. No source code was changed.

## Findings

### 1. High: material icon lookup can spawn network and cache-update processes from widget hot paths

Refs:

- `app/src/widgets/material_icon.rs:21`
- `app/src/widgets/material_icon.rs:61`
- `app/src/widgets/material_icon.rs:69`
- `app/src/widgets/material_icon.rs:89`
- `app/src/widgets/material_icon.rs:110`
- `app/src/widgets/material_icon.rs:138`
- `app/src/widgets/bar/mod.rs:363`
- `app/src/widgets/bar/project_label/mod.rs:73`
- `app/src/widgets/bar/project_label/mod.rs:233`
- `app/src/widgets/bar/window_tile/mod.rs:211`

`material_icon::icon_name` is used directly from watched GTK setters and helper
functions. Every call builds a resolved icon name and checks the filesystem for
the SVG. If the icon is missing, the running shell spawns a thread, invokes
`curl`, writes into the icon theme, runs `gtk-update-icon-cache`, and refreshes
the GTK icon theme.

Rationale: the one-shot `REQUESTED` set avoids repeated fetches for the same
missing icon, but this still allows runtime network/process work for each new
icon name. It also makes a UI helper do blocking filesystem checks on normal
render/update paths. In a shell process, missing icons during startup or state
changes can create visible latency, extra processes, and failure modes unrelated
to rendering.

Suggested fix: make icon resolution pure at runtime. Pre-bundle required
Material icons, generate them during install/build/dev tooling, or load a
process-local existence cache once at startup. If runtime fetching remains
useful for development, gate it behind an explicit dev feature or environment
flag, queue cache updates, and run `gtk-update-icon-cache` once per batch.

### 2. Medium: Locus relation sources create per-subject signal streams and repeated service reads

Refs:

- `app/src/widgets/bar/project_label/source/project.rs:35`
- `app/src/widgets/bar/project_label/source/project.rs:56`
- `app/src/widgets/bar/project_label/source/project.rs:65`
- `app/src/widgets/bar/project_label/source/project.rs:90`
- `app/src/widgets/bar/project_label/source/project.rs:120`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:60`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:85`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:94`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:119`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:149`

`project_details` starts one `from_task` source per workspace subject. Each
task opens a session-bus connection/proxy and subscribes separately to
`RelationAdded`, `RelationUpdated`, `RelationRemoved`, and `RelationCleared`.
On initial read and on each matching signal it calls `List` for the whole
workspace-project relation and scans for the subject.

The agent path has the same per-window-subject shape for `locus_targets`. It is
descriptor-shared by subject, which prevents duplicate work for the same active
window, but the shell still has one four-stream Locus watcher per active subject.

Rationale: the lifecycle scales with workspace/window count rather than with the
relation service. The repeated full `List` call in the project source is also
avoidable work when the signal body already carries relation records for add and
update cases.

Suggested fix: build one shared relation-index source per relation kind and
derive per-subject values from that shared map. For project labels, a single
`workspace_project_index()` could listen once, maintain `HashMap<Subject,
ProjectDetails>`, and expose per-workspace projection helpers. For agent
targets, use the same pattern or add a typed Locus DBus helper that shares the
relation stream at the service/relation descriptor level.

### 3. Medium: window snapshot fanout repeats full-list scans and sorts per workspace

Refs:

- `app/src/widgets/bar/workspaces.rs:49`
- `app/src/widgets/bar/workspaces.rs:59`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:21`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:38`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:39`
- `app/src/widgets/bar/project_label/source/agent.rs:15`
- `app/src/widgets/bar/project_label/source/agent.rs:20`
- `app/src/widgets/bar/project_label/source/agent.rs:24`
- `app/src/widgets/bar/project_label/source/agent.rs:34`

`window_snapshots()` is shared, but several consumers receive the whole
`Vec<WindowSnapshot>` and process it independently. The selected-workspace
window list retains and sorts the full snapshot. Every project label also
combines its workspace id with the full snapshot; the fallback source filters
and sorts for that workspace, and the agent summary filters into a new vector
before combining per-window agent sources.

Rationale: on every window snapshot update, work scales roughly with
`workspaces * windows`, with extra allocation and sorting. This is fine for a
small desktop, but it becomes avoidable churn when window movement, focus, or
metadata changes are frequent.

Suggested fix: compute a shared `windows_by_workspace` index once from
`window_snapshots()`, with per-workspace windows already sorted. Project-label
fallbacks, selected workspace tiles, and workspace agent aggregation can then
project from the shared index instead of re-filtering the global list.

### 4. Medium: audio status depends on a long-lived `pw-dump` subprocess and full JSON snapshots

Refs:

- `app/src/widgets/bar/audio/source.rs:41`
- `app/src/widgets/bar/audio/source.rs:59`
- `app/src/widgets/bar/audio/source.rs:78`
- `app/src/widgets/bar/audio/source.rs:104`
- `app/src/widgets/bar/audio/source.rs:116`
- `app/src/widgets/bar/audio/source.rs:127`
- `app/src/widgets/bar/audio/source.rs:132`
- `app/src/widgets/bar/audio/source.rs:249`

The audio source starts `pw-dump -m -N`, reads JSON chunks from stdout, parses
each chunk into `serde_json::Value`, retains the full PipeWire object set, and
rebuilds route/status snapshots on each update. `shared_by_key` keeps this to
one monitor per process, and `kill_on_drop(true)` is good lifecycle hygiene, but
the app still pays for an external monitor process plus untyped JSON allocation
on every PipeWire event.

Rationale: this is probably the heaviest steady-state widget source in the app.
It is also an FD/process lifecycle dependency that should be easy to observe and
restart if it exits.

Suggested fix: prefer a typed PipeWire/WirePlumber source when practical. If
`pw-dump` remains the pragmatic bridge, keep only the object classes and fields
needed for sink routes/default metadata, add tracing around monitor start/exit,
and consider a compact typed struct parse instead of retaining arbitrary
`serde_json::Value` trees.

### 5. Low: watched GTK setters allocate vectors, strings, and draw closures on frequent updates

Refs:

- `app/src/widgets/bar/mod.rs:321`
- `app/src/widgets/bar/mod.rs:330`
- `app/src/widgets/bar/mod.rs:341`
- `app/src/widgets/bar/mod.rs:345`
- `app/src/widgets/bar/mod.rs:369`
- `app/src/widgets/bar/mod.rs:380`
- `app/src/widgets/bar/mod.rs:384`
- `app/src/widgets/bar/project_label/mod.rs:44`
- `app/src/widgets/bar/project_label/mod.rs:56`
- `app/src/widgets/bar/project_label/mod.rs:107`
- `app/src/widgets/bar/project_label/mod.rs:192`
- `app/src/widgets/bar/project_label/mod.rs:250`
- `app/src/widgets/bar/project_label/mod.rs:267`
- `app/src/widgets/bar/window_tile/mod.rs:78`
- `app/src/widgets/bar/window_tile/mod.rs:88`
- `app/src/widgets/bar/window_tile/mod.rs:108`
- `app/src/widgets/bar/window_tile/mod.rs:122`
- `app/src/widgets/bar/window_tile/mod.rs:127`
- `app/src/widgets/bar/window_tile/mod.rs:165`
- `app/src/widgets/bar/window_tile/mod.rs:204`
- `app/src/widgets/bar/window_tile/mod.rs:233`
- `app/src/widgets/bar/bluetooth/mod.rs:204`
- `app/src/widgets/bar/bluetooth/mod.rs:212`
- `app/src/widgets/bar/bluetooth/mod.rs:220`

Several watch expressions construct new `Vec<&str>` class lists, formatted
strings, icon-name strings, and draw-function closures. The system stats widget
also calls `set_draw_func` again whenever CPU/RAM values change, even though
the draw style is stable and only the level changes.

Rationale: these are individually small allocations, but they sit in Relm/GTK
update paths. They can add up during source bursts, especially for repeated
rows such as project labels and window tiles.

Suggested fix: move display strings and icon names into view models where they
are computed once per source emission. Return static slices for class sets when
the variant set is small, or use small fixed arrays/`SmallVec` if dynamic
classes are needed. For level indicators, consider a tiny widget/model wrapper
that stores the level and queues redraw instead of replacing the draw closure on
every value change.

### 6. Low: periodic sources do more allocation than their display precision requires

Refs:

- `app/src/widgets/bar/system_stats/source.rs:30`
- `app/src/widgets/bar/system_stats/source.rs:51`
- `app/src/widgets/bar/system_stats/source.rs:58`
- `app/src/widgets/bar/system_stats/source.rs:73`
- `app/src/widgets/bar/time.rs:17`
- `app/src/widgets/bar/time.rs:29`
- `app/src/widgets/bar/time.rs:33`

The system stats source reads full `/proc/stat` and `/proc/meminfo` files every
three seconds, then allocates a vector for CPU fields. The clock source wakes
every second and formats both time and date strings, even though the displayed
time has minute precision and the date changes daily.

Rationale: this is low risk, but it is easy background churn in a shell process.
`distinct_until_changed` suppresses GTK redraws, but the source still wakes and
allocates every second for the clock.

Suggested fix: parse `/proc` with small reusable buffers or direct field
scanning instead of full-file strings plus a field vector. Drive the clock from
the next minute boundary, or split date and time sources so the date is not
formatted every second.

### 7. Low: user-triggered commands spawn unbounded detached threads

Refs:

- `app/src/widgets/bar/audio/source.rs:32`
- `app/src/widgets/bar/bluetooth/source.rs:62`
- `app/src/widgets/bar/bluetooth/source.rs:93`
- `app/src/widgets/bar/power_profile.rs:45`
- `app/src/widgets/bar/mod.rs:1060`

Audio route changes, Bluetooth connect/power actions, power-profile cycling,
and media controls each create a detached thread. Some of those threads build a
fresh current-thread Tokio runtime and D-Bus connection for a single method
call.

Rationale: this is not steady-state work, but repeated clicks can create many
simultaneous threads and commands with no backpressure or in-flight UI state.

Suggested fix: route command actions through a small app-owned async command
executor or Relm worker, reuse D-Bus connections where possible, and disable or
coalesce controls while the previous command is in flight.

## Positive Notes

- Most DBus property and ObjectManager helpers go through
  `shell_core::source::dbus`, which already uses descriptor-keyed
  `shared_by_key` for property, signal, ObjectManager, and model streams.
- `window_snapshots()` is shared at the source boundary, so the global Niri
  window property subscriptions are not duplicated by every downstream widget.
- The audio monitor is at least process-shared by key and uses `kill_on_drop`.
- Widget source providers stay local to their modules, matching the app widget
  guidance.

## Prioritized Action List

1. Remove or gate runtime Material icon fetching; make `material_icon::icon_name`
   a pure cached lookup in the running shell.
2. Replace per-subject Locus relation watchers with shared relation-index
   sources for workspace projects and window agent targets.
3. Add a shared `windows_by_workspace` derived source and use it for selected
   window tiles, project fallback icons, and workspace agent summaries.
4. Decide whether the PipeWire bridge should move to a typed native source; if
   not, reduce the retained JSON surface and add monitor lifecycle tracing.
5. Move frequently recomputed labels, tooltips, icon names, and class sets into
   view models, then avoid resetting draw functions for simple level changes.
6. Trim periodic background churn in clock and `/proc` stats sources.
7. Put user-triggered process/D-Bus commands behind a bounded async executor
   with in-flight state.
