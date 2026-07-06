# App Widget Performance Rerun

Review date: 2026-07-04

Scope: second-pass performance audit of the current uncommitted tree, focused
on `app/src/widgets` and widget-facing app support code. I read `AGENTS.md`,
`PROJECT.md`, `PLAN.md`, `SOURCE_API.md`, and `app/src/widgets/AGENTS.md`
before reviewing. No source code was changed.

## Findings

### 1. High: Material icon resolution still performs runtime filesystem, network, and process work from GTK update paths

Refs:

- `app/src/widgets/material_icon.rs:21`
- `app/src/widgets/material_icon.rs:61`
- `app/src/widgets/material_icon.rs:69`
- `app/src/widgets/material_icon.rs:89`
- `app/src/widgets/material_icon.rs:110`
- `app/src/widgets/material_icon.rs:123`
- `app/src/widgets/material_icon.rs:139`
- `app/src/widgets/bar/mod.rs:189`
- `app/src/widgets/bar/project_label/mod.rs:73`
- `app/src/widgets/bar/window_tile/mod.rs:88`

`material_icon::icon_name` is called directly by watched GTK setters and row
views. Each call formats a Material icon name and checks the icon file path; a
miss spawns a thread, runs `curl`, writes the SVG, runs
`gtk-update-icon-cache`, and refreshes the GTK icon theme.

Rationale: the per-icon `REQUESTED` set avoids duplicate downloads for one name,
but the shell can still start network and cache-update processes during startup
or ordinary widget changes. Even cache hits pay filesystem `exists()` checks in
render/update paths. This makes a small display helper a source of latency and
unrelated failure modes.

Suggested fix: make icon lookup pure in the running shell. Pre-bundle or
generate Material icons outside the UI process, or build a process-local
existence cache during startup. If runtime fetching is useful in development,
gate it behind an explicit dev flag and batch `gtk-update-icon-cache` outside
watch setters.

### 2. High: Locus relation watchers can miss updates between initial read and signal subscription

Refs:

- `app/src/widgets/bar/project_label/source/project.rs:63`
- `app/src/widgets/bar/project_label/source/project.rs:65`
- `app/src/widgets/bar/project_label/source/project.rs:90`
- `app/src/widgets/bar/project_label/source/project.rs:120`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:92`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:94`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:119`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:149`

Both app-local Locus relation loops emit an initial snapshot before installing
their `RelationAdded`, `RelationUpdated`, `RelationRemoved`, and
`RelationCleared` streams. A project or agent relation change in that gap can be
lost until a later matching signal happens.

Rationale: this is a latency/staleness bug in the app widget layer. The source
can appear live while project labels or agent indicators remain stale after a
single relation update.

Suggested fix: create all signal streams before the initial `List` / `Targets`
read, then emit the initial snapshot. Longer term, move both callers to one
shared relation-index source per relation kind so the ordering rule exists in
one helper.

### 3. Medium: Desktop app icon lookup does synchronous XDG scanning and repeated linear normalization on widget paths

Refs:

- `app/src/desktop_icon.rs:7`
- `app/src/desktop_icon.rs:10`
- `app/src/desktop_icon.rs:24`
- `app/src/desktop_icon.rs:38`
- `app/src/desktop_icon.rs:52`
- `app/src/desktop_icon.rs:61`
- `app/src/desktop_icon.rs:78`
- `app/src/desktop_icon.rs:137`
- `app/src/widgets/bar/window_tile/source.rs:60`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:55`

The first `desktop_icon::icon_for_app_id` call loads every `.desktop` file from
XDG application directories synchronously. Later lookups hold a mutex, scan the
full `entries` vector, and allocate normalized lowercase strings for the app id,
desktop id, and optional `StartupWMClass`.

Rationale: first-use lookup can block a source update on disk I/O during bar
startup or first window/project fallback rendering. Repeated lookups are linear
in installed desktop entries and are called from window tile view-model updates
and workspace fallback icon selection.

Suggested fix: build an indexed cache once, preferably off the GTK path:
`HashMap<normalized_app_key, icon>`, plus a small memo table for exact app-id
queries including misses. Store normalized fields in `DesktopIconEntry` if the
linear fallback remains useful.

### 4. Medium: Locus relation sources scale per active workspace/window subject

Refs:

- `app/src/widgets/bar/project_label/source/project.rs:35`
- `app/src/widgets/bar/project_label/source/project.rs:38`
- `app/src/widgets/bar/project_label/source/project.rs:56`
- `app/src/widgets/bar/project_label/source/project.rs:65`
- `app/src/widgets/bar/project_label/source/project.rs:125`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:60`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:64`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:85`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:94`
- `app/src/widgets/bar/window_tile/agent/source/actual.rs:155`

`project_details` creates one shared task per workspace subject, each with its
own session-bus connection/proxy and four relation signal streams. Agent target
lookup repeats the same shape per window subject. Project lookup also refreshes
by calling `List` for the whole relation and scanning for its subject.

Rationale: descriptor sharing prevents duplicate work for the same subject, but
the total steady-state work still scales with workspace/window count. It also
multiplies the missed-update race above and makes relation churn costlier than
needed.

Suggested fix: expose a shared app-local Locus relation helper keyed by relation
descriptor. Maintain a `HashMap<Subject, ProjectDetails>` or
`HashMap<Subject, Vec<Target>>` from one stream, then project per-workspace or
per-window observables from that index.

### 5. Medium: window snapshot fanout repeats full-list filtering, sorting, and per-window agent aggregation

Refs:

- `app/src/widgets/bar/window_source.rs:16`
- `app/src/widgets/bar/workspaces.rs:49`
- `app/src/widgets/bar/workspaces.rs:56`
- `app/src/widgets/bar/workspaces.rs:57`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:21`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:38`
- `app/src/widgets/bar/project_label/source/workspace_fallback.rs:39`
- `app/src/widgets/bar/project_label/source/agent.rs:15`
- `app/src/widgets/bar/project_label/source/agent.rs:20`
- `app/src/widgets/bar/project_label/source/agent.rs:24`

`window_snapshots()` is process-shared, but every downstream consumer receives
the whole vector. Selected workspace tiles retain and sort it. Each project
label repeats a retain/sort for fallback icons, and workspace agent state
filters the same global list before starting per-window agent projections.

Rationale: each Niri window update fans out into roughly
`workspaces * windows` filtering work, plus repeated sort allocations and
per-workspace aggregation. The current desktop size may hide this, but the work
is avoidable and compounds with `bind_list` reconciliation cost.

Suggested fix: derive a shared `windows_by_workspace` source from
`window_snapshots()`, with stable per-workspace ordering. Let selected tiles,
fallback icon selection, and workspace agent aggregation project from that
index.

### 6. Medium: audio status keeps a `pw-dump` process and arbitrary JSON object graph in the widget source

Refs:

- `app/src/widgets/bar/audio/source.rs:41`
- `app/src/widgets/bar/audio/source.rs:43`
- `app/src/widgets/bar/audio/source.rs:60`
- `app/src/widgets/bar/audio/source.rs:78`
- `app/src/widgets/bar/audio/source.rs:104`
- `app/src/widgets/bar/audio/source.rs:116`
- `app/src/widgets/bar/audio/source.rs:127`
- `app/src/widgets/bar/audio/source.rs:132`
- `app/src/widgets/bar/audio/source.rs:249`

The audio source starts `pw-dump -m -N`, parses each JSON chunk into
`serde_json::Value`, retains the full object set, merges updates into that tree,
and rebuilds status/routes by scanning all retained objects.

Rationale: this is likely the heaviest app widget source in steady state. It is
shared and `kill_on_drop(true)` is good, but it still carries process lifecycle,
untyped JSON allocation, O(n) update lookup, and full snapshot rebuild cost on
PipeWire events.

Suggested fix: prefer a typed PipeWire or WirePlumber source when practical. If
`pw-dump` remains the bridge, keep only the object classes/fields needed for
default sink and routes, index objects by id, and add tracing for monitor
start/exit/restart.

### 7. Low: render/update helpers allocate closures, class vectors, and geometry vectors

Refs:

- `app/src/widgets/bar/mod.rs:202`
- `app/src/widgets/bar/mod.rs:216`
- `app/src/widgets/bar/mod.rs:321`
- `app/src/widgets/bar/mod.rs:345`
- `app/src/widgets/bar/mod.rs:384`
- `app/src/widgets/bar/project_label/mod.rs:192`
- `app/src/widgets/bar/window_tile/mod.rs:152`
- `app/src/widgets/bar/window_tile/mod.rs:127`
- `app/src/widgets/bar/bzbus/view.rs:269`
- `app/src/widgets/bar/bzbus/view.rs:274`
- `app/src/widgets/bar/bzbus/view.rs:293`
- `app/src/widgets/bar/bzbus/view.rs:307`
- `app/src/widgets/level_indicator.rs:77`
- `app/src/widgets/level_indicator.rs:83`

Several watched setters rebuild class vectors and draw closures. The BzBus
perimeter draw path also allocates a `Vec<(f64, f64)>` and recomputes perimeter
length every draw.

Rationale: these costs are individually small, but they sit directly in GTK
update/draw paths and repeated row components. Source bursts can turn them into
visible allocation churn.

Suggested fix: move stable class sets and display strings into view models, use
static slices for small class variants, and avoid resetting draw functions when
only the level changed. For BzBus perimeter drawing, draw directly with Cairo or
cache geometry by `(width, height)`.

### 8. Low: periodic sources wake and allocate more often than displayed precision requires

Refs:

- `app/src/widgets/bar/time.rs:17`
- `app/src/widgets/bar/time.rs:18`
- `app/src/widgets/bar/time.rs:29`
- `app/src/widgets/bar/time.rs:33`
- `app/src/widgets/bar/system_stats/source.rs:30`
- `app/src/widgets/bar/system_stats/source.rs:52`
- `app/src/widgets/bar/system_stats/source.rs:58`
- `app/src/widgets/bar/system_stats/source.rs:74`

The clock wakes every second and formats both time and date, while the displayed
time has minute precision. System stats read full `/proc/stat` and
`/proc/meminfo` files and allocate a CPU-field vector every three seconds.

Rationale: this is low risk, but it is constant background churn in a shell
process. `distinct_until_changed` suppresses some model redraws but does not
avoid the source work.

Suggested fix: schedule the clock from the next minute boundary and split date
refresh to a daily boundary. Parse `/proc` with small fixed buffers or direct
field scanning instead of full-file strings plus a `Vec`.

### 9. Low: user-triggered actions spawn detached threads without backpressure

Refs:

- `app/src/widgets/bar/audio/source.rs:32`
- `app/src/widgets/bar/bluetooth/source.rs:51`
- `app/src/widgets/bar/bluetooth/source.rs:58`
- `app/src/widgets/bar/bluetooth/source.rs:85`
- `app/src/widgets/bar/bluetooth/source.rs:89`
- `app/src/widgets/bar/power_profile.rs:45`
- `app/src/widgets/bar/power_profile.rs:48`
- `app/src/widgets/bar/mod.rs:948`
- `app/src/widgets/bar/mod.rs:1060`

Audio route changes, Bluetooth actions, power-profile cycling, notification
center toggling, and MPRIS controls each spawn detached threads. The D-Bus
actions build a fresh current-thread Tokio runtime and bus connection for one
method call.

Rationale: these are user-triggered rather than steady-state paths, but rapid
clicks can create unbounded concurrent work with no in-flight state or
coalescing.

Suggested fix: add a small app-owned command executor for widget actions. Reuse
D-Bus connections where practical, log outcomes uniformly, and disable or
coalesce controls while a previous command is pending.

## Positive Notes

- Base Niri window snapshots, battery, network, Bluetooth, power profile, and
audio sources are generally descriptor-shared instead of duplicated per view.
- Most widget providers emit distinct view models before reaching Relm4.
- Popover-heavy Bluetooth route lists are mounted lazily rather than kept alive
for the entire bar lifetime.
- Source providers remain local to their widgets, matching the app widget
guidance.

## Prioritized Action List

1. Remove or dev-gate runtime Material icon fetching and cache icon existence
   outside GTK watch paths.
2. Fix app-local Locus relation watcher ordering: subscribe first, then read
   initial snapshots.
3. Replace per-subject Locus tasks with shared relation-index sources.
4. Index desktop app icons by normalized app key and memo exact app-id lookups.
5. Add a shared `windows_by_workspace` derived source for selected tiles,
   project fallbacks, and workspace agent summaries.
6. Reduce the PipeWire bridge to typed/indexed state or replace it with a typed
   source.
7. Trim render-path allocation in draw/class/icon helpers.
8. Reduce periodic clock and `/proc` churn.
9. Route widget commands through a bounded app action executor.
