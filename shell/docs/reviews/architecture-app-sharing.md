# App Architecture Sharing Review

Scope: app-local sharing opportunities in `app/`, especially code that can be
shared within widgets or extracted into reusable app modules while preserving
widget-local source providers and avoiding a top-level `src/sources` module.

## Findings

### P1 - Extract app-local typed DBus service helpers

Several widget-local providers define the same descriptor wrapper shape: service
constants, typed object handles, `ObjectModel` impls, `object(...)`, and
`property_or(...)` helpers. Examples:

- Niri uses typed workspace/window handles plus local `required`, `optional`,
  `model_optional`, `object`, and ObjectManager helpers in
  `app/src/widgets/bar/niri.rs:14`, `app/src/widgets/bar/niri.rs:116`, and
  `app/src/widgets/bar/niri.rs:134`.
- BlueZ repeats object/property helpers and object models in
  `app/src/widgets/bar/bluetooth/model.rs:13`,
  `app/src/widgets/bar/bluetooth/model.rs:99`, and
  `app/src/widgets/bar/bluetooth/model.rs:104`.
- NetworkManager has the same service-object pattern in
  `app/src/widgets/bar/network/model.rs:18`,
  `app/src/widgets/bar/network/model.rs:118`, and
  `app/src/widgets/bar/network/model.rs:196`.
- UPower and PowerProfiles repeat single-object descriptor helpers in
  `app/src/widgets/bar/battery.rs:47`, `app/src/widgets/bar/battery.rs:65`,
  and `app/src/widgets/bar/power_profile.rs:33`.
- AgentDBus session helpers repeat the typed object/property shape in
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:35`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:214`, and
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:251`.

What to share: service-specific typed client modules under `app/src/services/`
or `app/src/dbus/`, for example `app::services::{niri, bluez,
network_manager, upower, power_profiles, agent_dbus}`. These modules should
own typed object descriptors and property helpers only; widget source providers
should remain beside their widgets and compose those typed helpers into widget
view models.

Target: `app` crate, not `shell-core`, unless a primitive is truly generic D-Bus
transport behavior. Niri, BlueZ, NetworkManager, UPower, PowerProfiles, and
AgentDBus are product integration policy.

Risk: moving too much source composition would violate widget-local source
rules. Keep reusable modules at descriptor/client level and leave view-model
sources such as `bluetooth_status`, `network_status`, and `project_label_vm`
near widgets. Also avoid hand-writing generated Locus schema APIs.

### P1 - Extract a Locus relation-watch helper

Two providers independently implement the same Relation service watch loop:

- Workspace project source defines Locus bus constants and `RelationRecord` in
  `app/src/widgets/bar/project_label/source/project.rs:12`, then connects a
  session proxy, sends an initial result, subscribes to `RelationAdded`,
  `RelationUpdated`, `RelationRemoved`, and `RelationCleared`, and refreshes on
  matching records in `app/src/widgets/bar/project_label/source/project.rs:52`.
- Window agent source repeats the constants, record shape, signal wiring,
  `relation_record_matches`, `clear_matches`, `to_string`, and
  `is_locus_unavailable` in
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:20`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:80`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:173`, and
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:292`.

What to share: an app-local `services::locus_relations` helper that exposes
Observable-friendly functions such as `targets(subject, relation)` and maybe
`records(relation)` or `first_record(subject, relation)`. It can own the
common D-Bus proxy, relation record DTO, signal matching, ServiceUnknown
fallback, and `shared_by_key` key format.

Target: `app` crate service module. Do not put this in `shell-core`; the Locus
relation service is product/sibling-service specific. Do not turn it into a
provider facade or schema-extension layer.

Risk: the two callers currently need different payloads. The agent source uses
the `Targets` method for a subject/relation pair
(`app/src/widgets/bar/window_tile/agent/source/actual.rs:149`), while project
details uses `List` then maps metadata
(`app/src/widgets/bar/project_label/source/project.rs:120`). A shared helper
should expose relation records and/or targets without embedding widget display
policy.

### P1 - Centralize workspace window snapshot filtering and ordering

The bar already has a shared `window_snapshots()` source, but each consumer
still repeats filtering and ordering logic:

- `selected_workspace_windows()` filters snapshots by workspace and sorts by
  `(column, row, id, path)` in `app/src/widgets/bar/workspaces.rs:49`.
- Project label fallback repeats the same retain/sort/path tie-breaker in
  `app/src/widgets/bar/project_label/source/workspace_fallback.rs:27`.
- Workspace agent state filters snapshots for a workspace in
  `app/src/widgets/bar/project_label/source/agent.rs:15`.

What to share: keep `window_snapshots()` in `app/src/widgets/bar/window_source.rs`
and add bar-local helpers there, such as `windows_for_workspace_id(...)`,
`workspace_snapshots(workspace)`, and `compare_window_snapshots(...)`. These are
bar source utilities, not a top-level source module.

Target: `app/src/widgets/bar/window_source.rs` or a private sibling under
`app/src/widgets/bar/window_source/` if the file grows.

Risk: ordering is user-visible in the bar and project fallback icon selection.
Any extraction should keep the current sort stable and should add focused tests
for selected-window ordering and fallback icon selection.

### P2 - Extract async command/action runners for app commands

Multiple app actions spawn background threads and then either run a process or
create a one-off Tokio runtime for D-Bus calls:

- PowerProfiles `set_property` creates a thread and current-thread Tokio
  runtime in `app/src/widgets/bar/power_profile.rs:45`.
- BlueZ power and connect/disconnect do the same in
  `app/src/widgets/bar/bluetooth/source.rs:55` and
  `app/src/widgets/bar/bluetooth/source.rs:89`.
- Audio default route shells out to `wpctl` on a thread in
  `app/src/widgets/bar/audio/source.rs:32`.
- Notification-center toggle spawns a thread around request sending in
  `app/src/widgets/bar/mod.rs:948`.
- MPRIS controls shell out to `playerctl` in
  `app/src/widgets/bar/mod.rs:1060`.
- Theme accent sync shells out synchronously in `app/src/theme.rs:101`.

What to share: an app-local `actions` or `command` helper with small primitives
such as `spawn_logged(label, FnOnce -> Result<(), String>)`, `spawn_process`,
and `spawn_dbus_system(label, async fn)`. Service modules can then expose typed
actions (`bluez::set_adapter_power`, `power_profiles::set_active_profile`) and
widget update handlers remain thin.

Target: `app` crate. The Unix request bridge and command names stay in app, as
required by project guidance.

Risk: a global runtime or generic command bus would be too broad. Keep this as
execution boilerplate and error reporting only. Be careful not to block GTK for
`theme::sync_accent`; either leave it synchronous if intentionally startup-only
or move it through the same app action runner with explicit lifecycle handling.

### P2 - Share popover component mounting and lazy lifecycle helpers

`MainBar::init` has generic component mounting logic for popovers, then
special-case wrappers for audio and Bluetooth:

- Audio creates a popover, mount box, controller cell, and visible-notify hook
  in `app/src/widgets/bar/mod.rs:794`.
- `mount_popover_component` is generic and not bar-specific except for its
  local placement in `app/src/widgets/bar/mod.rs:978`.
- Bluetooth wraps the same lifecycle in `mount_bluetooth_group_popover` at
  `app/src/widgets/bar/mod.rs:1004`.
- Audio row reaches upward through widget ancestry to close its popover in
  `app/src/widgets/bar/audio/route_row.rs:72`.

What to share: a widget utility under `app/src/widgets/popover.rs` or
`app/src/widgets/common/popover.rs` that owns lazy `Component` mounting on
popover visibility and a small helper to close the containing popover from a
row. This is reusable UI behavior, not source behavior, so it does not violate
widget-local source rules.

Target: `app::widgets`, potentially re-exported from `app/src/widgets/mod.rs`.

Risk: component controller lifetimes are easy to break. Preserve the current
mount/remove behavior and only abstract the repeated lifecycle. Avoid nesting
policy such as "Bluetooth group popover" into the shared helper.

### P2 - Factor repeated route/list row primitives

The app has several list rows and list popovers with the same visual structure:

- Audio route popover is a vertical list bound with `#[bind_list]` in
  `app/src/widgets/bar/audio/route_popover.rs:22`.
- Audio route row is a clickable row with material icon, title, subtitle, and
  optional trailing check in `app/src/widgets/bar/audio/route_row.rs:18`.
- Bluetooth group popover is another vertical `#[bind_list]` in
  `app/src/widgets/bar/bluetooth/mod.rs:77`.
- Bluetooth device row is an activatable row with icon, title, subtitle, and a
  click handler in `app/src/widgets/bar/bluetooth/mod.rs:107`.
- Source error popover uses a vertical bound list in
  `app/src/widgets/bar/mod.rs:680` with row layout in
  `app/src/widgets/bar/source_errors.rs:29`.

What to share: app widget primitives for `PopoverList`, `IconTextRow`, and
possibly an `ActionRow` adapter. Keep concrete source providers and row view
models local; share only GTK layout conventions and row behavior that is
already duplicated.

Target: `app/src/widgets/common/` or narrowly `app/src/widgets/list.rs`. Avoid
putting product-specific rows in `shell-core`; `shell-core::list` already owns
generic list binding support.

Risk: over-abstraction can make Relm4 `view!` code harder to read. Start with
small constructors/helpers for common classes, icon/title/subtitle shape, and
popover list mounting. Keep row components local when they have meaningful
actions.

### P2 - Add app-level icon semantics around material/app icons

Icon naming and conversion policy is spread across widgets:

- `material_icon::icon_name` resolves and fetches Material Symbols in
  `app/src/widgets/material_icon.rs:21`.
- Desktop app icon lookup lives separately in `app/src/desktop_icon.rs:7`.
- Project labels choose between app icon classes and material icon names in
  `app/src/widgets/bar/project_label/mod.rs:224`.
- Window tiles choose between app icons and material icons for agent windows in
  `app/src/widgets/bar/window_tile/mod.rs:204`.
- Many bar controls inline `material_icon::icon_name(...)`, for example
  `app/src/widgets/bar/mod.rs:187`, `app/src/widgets/bar/mod.rs:279`,
  `app/src/widgets/bar/mod.rs:359`, and `app/src/widgets/bar/mod.rs:667`.

What to share: an app-local icon model such as `IconRef::Theme(String)` /
`IconRef::Material(&'static str)` / `IconRef::App(String)` plus helper methods
for GTK icon name and CSS class list. Widgets can still decide which icon is
semantically correct, but rendering no longer repeats "is this an app icon or a
material icon" policy.

Target: `app::widgets::icons` or extend `app::widgets::material_icon` into an
`icons` module. Keep network/download behavior inside the material icon
implementation.

Risk: app icon lookup and material icon fetching have side effects and caches.
Do not move them into render hot paths without preserving current caching.
Also keep plain `*-symbolic` theme icons untouched; not every icon should be
treated as Material.

### P3 - Share small formatting helpers only after service helpers land

There are many tiny `non_empty`, percent, fallback, and tooltip helpers:

- `non_empty` appears in `app/src/widgets/bar/window_source.rs:52`,
  `app/src/widgets/bar/window_tile/source.rs:96`,
  `app/src/widgets/bar/project_label/source.rs:72`,
  `app/src/widgets/bar/project_label/mod.rs:281`, and
  `app/src/widgets/bar/bzbus/view.rs:450`.
- Percent formatting appears in `app/src/widgets/bar/audio/source.rs:345`,
  battery tooltip/icon handling in `app/src/widgets/bar/mod.rs:1023`, and
  BzBus progress/duration formatting in `app/src/widgets/bar/bzbus/view.rs:232`
  and `app/src/widgets/bar/bzbus/view.rs:434`.
- Tooltip builders remain widget-specific, for example network at
  `app/src/widgets/bar/network/mod.rs:144`, Bluetooth at
  `app/src/widgets/bar/bluetooth/view.rs:113`, and project labels at
  `app/src/widgets/bar/project_label/mod.rs:267`.

What to share: a tiny app-local text module for obvious pure helpers
(`trimmed_non_empty`, `percent_label`, maybe `duration_mmss`) only where it
removes real repetition. Keep domain tooltip wording beside the widget.

Target: `app/src/text.rs` or `app/src/widgets/format.rs`, depending on whether
callers stay entirely under widgets.

Risk: this is lower priority because premature helper extraction can hide
domain-specific display policy. It becomes more valuable after DBus and
relation helper extraction reduces the larger duplication.

### P3 - Extend the level indicator primitive for perimeter progress

`widgets::level_indicator` already centralizes line/arc indicator drawing,
stage classes, track classes, and Cairo color setup in
`app/src/widgets/level_indicator.rs:75` and
`app/src/widgets/level_indicator.rs:87`. System stats, Bluetooth battery, and
agent context use it through `app/src/widgets/bar/system_stats/mod.rs:50`,
`app/src/widgets/bar/bluetooth/mod.rs:212`, and
`app/src/widgets/bar/window_tile/mod.rs:233`.

BzBus progress keeps a custom rounded-perimeter progress renderer with its own
track classes, color helper, polyline fraction, and perimeter point generation
in `app/src/widgets/bar/bzbus/view.rs:261`,
`app/src/widgets/bar/bzbus/view.rs:282`, and
`app/src/widgets/bar/bzbus/view.rs:384`.

What to share: add a `LevelRenderStyle::Perimeter` or a separate
`progress_frame` primitive under `app::widgets` if another widget needs the
same perimeter progress. At minimum, share Cairo color/fraction helpers if
perimeter drawing grows.

Target: `app/src/widgets/level_indicator.rs` if it remains a generic level
render style; otherwise `app/src/widgets/progress_frame.rs`.

Risk: this is currently only one caller, so defer until another perimeter-style
indicator appears or BzBus rendering needs tests/refinement. Do not move this
to `shell-core`; it is visual product UI.

## Suggested Extraction Order

1. Extract `services::locus_relations` first. It removes the most fragile
   duplicated async signal code and reduces risk around relation lifecycle bugs.
2. Extract app-local typed DBus service modules next, starting with Niri and
   BlueZ because they already have multiple consumers/actions.
3. Consolidate `bar::window_source` workspace filtering and ordering with
   tests.
4. Move popover mounting and lightweight row/list primitives under
   `widgets`.
5. Add icon/text/indicator refinements only where follow-up edits touch those
   widgets.
