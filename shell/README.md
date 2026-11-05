# Rsynapse Shell

`shell/` is the Rsynapse shell UI monorepo. It contains reusable Rust
GTK4/Relm4 framework crates plus concrete Rsynapse UI surfaces.

## Layout

- `core/shell-core`
  Generic app startup, stylesheet loading, and layer-shell window setup.
  Re-exports the Observable source primitives from `core/shell-source`.

- `core/shell-source`
  UI-free Observable source primitives (D-Bus properties, signals,
  ObjectManager snapshots, Rx composition helpers) for widgets and
  headless consumers alike.

- `core/macros`
  Relm4 model/source binding procedural macros.

- `core/rx-macros`
  Small RxRust composition macros.

- `widgets/nerd-icon-picker`
  Reusable `pick-icon`-backed fuzzy GTK picker for Nerd Font glyphs and
  consumer-supplied specific icon rows.

- `remarked` (`rsynapse-workspace`)
  Fresh **Tasks & Notes** layer-shell overlay, hosted inside the existing shell process.
  It composes projd tasks and Remarked notes through shell-source Observables and
  uses a native GtkSourceView Markdown editor. Click the bar's goal widget, press
  **Mod+N** in Umbriel, or run `rsynapse-shell request workspace show|hide|toggle`.
  Project notebooks live in globally Git-ignored `<project-cwd>/.notes/` directories;
  immutable-instance `task/<id>` frontmatter tags associate notes with tasks.
  Escape dismisses the centered overlay, releasing its keyboard focus. It does
  not create or resize a normal tiled window.
  See [the surface guide](remarked/README.md).

- `app`
  The current combined `rsynapse-shell` package. It owns the bar, OSD,
  notifications bridge, request socket, styles, and Rsynapse-specific UI
  policy.
   Clicking the workspace title or using the compositor shortcut opens a
   centered, keyboard-first Adwaita layer-shell workspace selector for the
   focused workspace. The search field filters registered projects; arrow keys
   move the selection and Enter applies it. Typing a name also appends a
   **Rename to …** row, still selected with the arrows and applied with Enter.
   The current project is marked and selected first, and an **Unassign project**
   row appears while the workspace has an association. Pressing Delete on a
   project row asks for confirmation before deleting project metadata; the
   checkout and its files are not touched. The previous workspace popover and
   inline editor have been removed.
   Selecting a project assigns it through steward; project files and directories
   are untouched. Renaming stores an explicit persistent manual workspace name.
   Each checkout/worktree is a separate project.
   `rsynapse-shell request workspace-selector show|hide|toggle` controls the
   selector (alias `workspace-picker`); bind
   `spawn:$HOME/.local/bin/rsynapse-shell request workspace-selector toggle` in
   Umbriel. `rsynapse-shell request project-init` keeps opening the directory
   picker for registering a new project checkout.
    Associated icons subscribe to the project's initial/live `Icon` property;
    changing the association switches subscriptions. Only unset icons use shell
    heuristics, persisted conditionally so manual selections always win. Project
    names/icons persist in projd independently for each worktree. Unassociated
    workspace icon overrides remain persistent locus relations.
   Automatic titles use the stored `Project.Name`, or `empty` without a
  project; preferred names are preserved by steward.
  Unassigned workspace buttons have a leading folder-plus icon.
   The bar also shows the project's active goal as a button (or **Choose goal**).
   The button shows only the current subgoal title when one is being worked on,
   otherwise the main goal title. Parent context stays in the tooltip.
   A selected parent follows its unique in-progress child; multiple in-progress
   siblings require an explicit selection instead of guessing the current step.
   The button remains available for associated projects with no goals, so it can
   open the overlay to create the first task.
   The main bar uses a CenterBox: windows on the left, a context block in the
   middle, and status widgets on the right. The context block is itself a
   CenterBox, with project/workspace title plus Git information in its center
   slot. Its start slot expands to fill the left section, with the goal
   right-aligned inside that section, next to the centered project/Git row.
   The goal is an unmeasured overlay within the left section, so long
   titles shrink to available space instead of displacing the center row.
   Clicking the goal widget opens the shell-hosted Tasks & Notes overlay directly;
   task management and note editing live there. There is no separate notes icon
   or task popover in the bar. Titles/descriptions and selection follow projd
   properties, while task lists use reactive service snapshots rather than
   Markdown parsing. Active tasks can also be selected with `proj goal start`
   or `proj goal active`; cancelled tasks are closed and cannot remain selected.

- `launcher`
  The launcher workspace. It owns the D-Bus launcher daemon, CLI, GTK launcher
  UI, plugin API, and bundled plugins.

## Architecture

Shell UI state is D-Bus-first:

```text
D-Bus services -> zbus streams -> shell_core::source::Observable<T> -> Relm4
```

Framework crates must stay generic. Product behavior, widget view models,
styling, request commands, and launcher policy belong in consumer crates.

The bar consumes the independent `workspace-dbus-api` contract at
`org.rsynapse.Workspace`. Workspace/window IDs are opaque strings read from
properties; object paths are addresses, never parsed as IDs. Niri/Umbriel
protocols and normalization stay in the projection service. Steward owns Locus
association cleanup; project/goal data stay in projd.

Window tiles, the workspace rail, tray items, GitHub rows, and other
`#[bind_list]` surfaces reconcile through the shell-core diff list host. Equal
row components are retained across insertions, deletions, and reorders instead
of rebuilding the whole list. The workspace selector uses the same diff logic
through `source::diff`.

The bar's corner indicator follows its output's selected workspace `Layout`
property, including live changes and workspace switches. Scrolling retains the
move-arrows glyph; Master uses a left-docked panel and Dwindle a tiled dashboard.
The tooltip names the layout. Unknown/missing layouts use a neutral indicator.

When the primary monitor disappears, the bar and rail move to a live monitor and
their output-scoped window, project/picker, goal and layout subscriptions switch
together. Reconnecting a display under the same connector refreshes the GDK
monitor binding; the replacement primary is removed from secondary controllers.
An output temporarily absent from the projection yields an empty rail rather
than falling back to other monitors' workspace targets.

Agent icons in the top window strip have slim vertical context-usage meters,
using the same level renderer as CPU/RAM in line mode. Each follows its own
AgentDBus session's `ContextPct`; the window tooltip includes the percentage.
Missing or nonfinite readings hide the meter without hiding the agent icon.
The meter warns at 80%, becomes danger-colored at 90%, and critical at 95%.
Reported zero remains an empty track, including providers whose AgentDBus
context telemetry has not been populated yet.

The left vertical rail is a vertical CenterBox: GitHub PR status at the top,
ongoing build indicators in the center, and workspace controls at the bottom.
GitHub controls form two distinct lighter themed pills with icon-only headers:
a **pull-request icon** for your PR status and the **GitHub icon** for GitHub
activity (review requests and your outstanding changes requests). Both use always-expanded vertical
revealers; the activity block is hidden when it has no items. Header names are
available as tooltips rather than visible text.
The groups do not collapse on hover or owner clicks; individual status buttons
retain their own PR popovers.
GitHub and Bluetooth use the same `widgets::button_group::ButtonGroup` GTK
widget. Its configuration derives orientation and owner/content ordering from
the expansion direction (left/right/up/down), with always-open or hover reveal
policies. Bluetooth expands left on hover and stays open while any registered
device popover is visible. Owners fill the cross axis; pill styling is an
optional CSS class rather than a separate container implementation.
The GitHub section contains independent review-requested, merging, recently merged, failed CI/merge, CI-pending,
approved/ready, To address, conflict, changes-requested, needs-review and draft
items. Zero-count items are hidden. Each item has its own popover listing
matching PR titles, repositories, numbers and statuses. Every PR belongs to
exactly one segment: merged/merging and explicit conflict/change-request/failure
states take priority over unresolved threads; unresolved threads take priority
over waiting/ready/draft states. When feedback is resolved, the PR moves to its
current review, CI or ready state. Needs review includes all required approvals,
including OWNERS, without a separate owner-approval segment. Secondary thread
details remain in each row. Counts are PR counts, not totals of review threads.
Failed check rollups (`FAILURE`/`ERROR`) also outrank waiting/ready states, even
when the service's primary status still indicates an approval requirement.

The GitHub activity block has three exclusive review-workflow items:

- **Needs your initial review**: pending request, no submitted review by you.
- **Needs your re-review**: pending request after a previous submitted review,
  including a previous approval or changes request.
- **Changes requested by you**: your outstanding changes request, waiting for
  the author, even without a pending request for your review.

github-dbus reads viewer-filtered submitted review history; opening a PR or
marking it seen never moves it between workflow items. Approved reviews without
a new request and comment-only inactive reviews are excluded. A new explicit
request after approval reappears as re-review. A comment-only follow-up does
not clear an outstanding changes decision; approval or dismissal does.
Underlying CI/thread details stay in each row. The authored block's awaiting
approval/changes-requested states are separate from your reviewer workflow.
All three review-workflow items use an independent **tagless Listener**, shared
across monitors. They remain global when switching projects, changing labels,
or while project context is unavailable. Project labels filter only your authored
PR block, never work requesting your review or waiting for the author.
Archived repositories are excluded by github-dbus before publication, for both
authored PRs and review activity, so neither block displays them.

PR popovers measure their content's natural width instead of using a fixed
minimum or a narrow title-width hint. Text can still wrap when available space
constrains the popup.

Clicking a PR opens its URL through an external browser opener, avoiding GTK URI
parent export from the layer-shell popup. PR rows and category buttons use the
same small unseen badge as workspace/agent indicators, and numeric counters use
standard panel badge CSS. Opening a list does not mark changes seen: opening the
individual PR acknowledges its displayed revision after a successful launch.
Right-clicking a PR row marks that displayed revision seen without opening it.
Unseen state persists in github-dbus and updates all monitors together.
Recently merged PRs act as a completion inbox: they remain unseen until opened
or explicitly marked seen, then disappear from the panel and every published
PR inventory property. Persisted dismissal IDs prevent refreshes/restarts from
bringing them back.

Counts, lists and unseen flags use one live `PullRequestActivity` snapshot
through typed shell-source observables, with owner-replacement recovery. The
snapshot uses Rsynapse-owned client schema types; the D-Bus integration has no
build dependency on the service's private Rust API crate. Each
rail's authored block reuses its output's selected-project context. A project
with labels shows your PRs matching **any** label; no project or no labels shows
all your PRs. A labeled
project keeps its filter even on a workspace with no windows. A configured
filter with no matches stays empty; unavailable labels are not an empty filter.

Membership comes from github-dbus Listener objects, not shell-side label
matching. The factory matches all requested labels, so the shell unions one
singleton-label listener per project label (deduplicating URLs). An empty-label
listener provides global membership. Counts, rows and unseen badges join those
URLs to the shared activity snapshot before exclusive category classification.
The review block separately joins the same activity snapshot to a tagless
listener, without observing project labels. If the authored block also uses an
empty-label filter, both paths share the same underlying remote listener lease.
Property changes update membership without Refresh calls or polling. Equivalent
filters share listener leases across rails, and final unsubscribe closes the
listener on its creator connection. Service replacement recreates listeners.
The total/open item is omitted.

The Project popover has a live **Labels** section with Add/Enter and individual
remove actions. CLI/agent changes appear there immediately. Labels are generic
projd metadata; the editor uses atomic add/remove methods and captures its target
project so reassociation cannot redirect an in-flight edit. From a project CWD:

```sh
proj project labels get --json
proj project labels add "feature-label" "team-label"
proj project labels remove "team-label"
```

The workspace rail hides only unselected, anonymous empty edge placeholders
without project/icon evidence. Empty interior workspaces and named workspaces
remain visible; the selected workspace always stays represented. Occupancy comes
from the workspace's window inventory rather than application-ID/icon heuristics.

## Common Commands

For Nix-provided GTK/Rust dependencies, enter `nix develop` from the repository
root. The root flake replaces the former launcher `shell.nix` and Nix-store
discovery workaround. `rebuild.sh` uses normal Cargo to install both shell
binaries under `${PREFIX:-$HOME/.local}`, then restarts their user units.

From this directory:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo test --workspace
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo fmt --check
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-shell
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-notifications
```

Launch either UI with the GTK inspector opened at startup by passing `inspect`:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-shell -- inspect
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-notifications -- inspect
```

Stop the corresponding systemd user service first when it is already running;
GTK applications use a single instance per application ID.

The launcher is a nested workspace:

```sh
cd launcher
cargo test --workspace
cargo run -p rsynapse-daemon
cargo run -p rsynapse-cli -- search firefox
```

## More Detail

- `PROJECT.md` describes the shell framework design and constraints.
- `PLAN.md` tracks the live roadmap.
- `SOURCE_API.md` describes the Observable-first source API.
- `AGS_REFERENCE.md` records product behavior to preserve from the old AGS
  shell without treating that implementation as an architecture template.
