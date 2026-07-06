# App Architecture Sharing Review Rerun

Scope: second-pass app-level sharing audit for the current uncommitted tree.
The review focuses on code that can be shared inside `app/` while preserving
widget-local source providers and avoiding a top-level `src/sources` module.

Read before audit: `AGENTS.md`, `PROJECT.md`, `PLAN.md`, `SOURCE_API.md`, and
`app/src/widgets/AGENTS.md`.

## Findings

### P1 - Extract app-local typed DBus service helpers

The current app widgets still repeat the same typed DBus descriptor pattern:
bus constants, object handles, `ObjectModel` impls, descriptor constructors,
and `property` / `property_or` wrappers.

Evidence:

- Niri owns workspace/window handles and local descriptor helpers in
  `app/src/widgets/bar/niri.rs:14`, `app/src/widgets/bar/niri.rs:53`,
  `app/src/widgets/bar/niri.rs:108`, `app/src/widgets/bar/niri.rs:134`, and
  `app/src/widgets/bar/niri.rs:169`.
- BlueZ repeats object models and property helpers in
  `app/src/widgets/bar/bluetooth/model.rs:13`,
  `app/src/widgets/bar/bluetooth/model.rs:32`,
  `app/src/widgets/bar/bluetooth/model.rs:91`,
  `app/src/widgets/bar/bluetooth/model.rs:99`, and
  `app/src/widgets/bar/bluetooth/model.rs:104`.
- NetworkManager repeats the same service-object shape in
  `app/src/widgets/bar/network/model.rs:18`,
  `app/src/widgets/bar/network/model.rs:52`,
  `app/src/widgets/bar/network/model.rs:192`, and
  `app/src/widgets/bar/network/model.rs:205`.
- UPower and PowerProfiles use local single-object descriptors in
  `app/src/widgets/bar/battery.rs:47`,
  `app/src/widgets/bar/battery.rs:65`,
  `app/src/widgets/bar/power_profile.rs:33`, and
  `app/src/widgets/bar/power_profile.rs:78`.
- AgentDBus repeats session object/property helper code in
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:214` and
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:251`.

Target: add app-local service modules such as `app/src/services/niri.rs`,
`bluez.rs`, `network_manager.rs`, `upower.rs`, `power_profiles.rs`, and
`agent_dbus.rs`, re-exported through `app/src/services/mod.rs`. These modules
should own typed handles, constants, descriptor construction, and typed service
actions. Widget-local source files should still compose those handles into
`Observable<ViewModel>` values beside their widgets.

Risk: extracting view-model composition into services would violate widget
locality. Keep product display policy such as `BluetoothView`, `NetworkView`,
`ProjectLabelVm`, and `Agent` mapping in the widget modules. Do not move this
to `shell-core`; these are Rsynapse product integrations, not generic DBus
transport primitives.

Verification: descriptor construction tests for each service module plus the
existing widget source tests.

### P1 - Extract a Locus relation service helper

Two widget providers independently implement nearly identical Locus relation
watch loops.

Evidence:

- Project labels define Locus constants and `RelationRecord` in
  `app/src/widgets/bar/project_label/source/project.rs:12` and
  `app/src/widgets/bar/project_label/source/project.rs:17`, then open the
  session bus, subscribe to `RelationAdded`, `RelationUpdated`,
  `RelationRemoved`, and `RelationCleared`, and refresh on matching signals in
  `app/src/widgets/bar/project_label/source/project.rs:52`.
- Window agent lookup repeats the same constants, record shape, signal setup,
  relation matching, clear matching, and `ServiceUnknown` fallback in
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:20`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:25`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:80`,
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:173`, and
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:292`.
- The only meaningful behavioral split is the read method: project labels call
  `List` and preserve metadata in
  `app/src/widgets/bar/project_label/source/project.rs:120`, while agent lookup
  calls `Targets` in
  `app/src/widgets/bar/window_tile/agent/source/actual.rs:149`.

Target: add `app/src/services/locus_relations.rs` with shared relation DTOs,
proxy construction, signal matching, `ServiceUnknown` handling, and
Observable-friendly helpers such as `records(relation)` and
`targets(subject, relation)`. Widget providers should keep mapping relation
records into `ProjectDetails` or `Agent` state locally.

Risk: a too-broad helper could become the removed provider facade in another
form. Keep it as a typed app service helper around the Locus D-Bus API, not a
graph schema layer and not a generated-style path API.

Verification: pure tests for record matching and clear matching, plus fallback
behavior when Locus is unavailable.

### P1 - Centralize bar window snapshot filtering and ordering

`window_snapshots()` is already shared, but its consumers repeat workspace
filtering and window ordering.

Evidence:

- `WindowSnapshot` is produced centrally in
  `app/src/widgets/bar/window_source.rs:6`, but consumers sort on
  `(column, row, id)` with path-key tie-breaking themselves.
- Selected workspace windows filter and sort in
  `app/src/widgets/bar/workspaces.rs:46`.
- Workspace fallback repeats the same retain/sort order in
  `app/src/widgets/bar/project_label/source/workspace_fallback.rs:27`.
- Workspace agent state repeats workspace filtering in
  `app/src/widgets/bar/project_label/source/agent.rs:15` and
  `app/src/widgets/bar/project_label/source/agent.rs:34`.

Target: keep this under the bar, for example in
`app/src/widgets/bar/window_source.rs` or a private
`app/src/widgets/bar/window_source/` directory. Add helpers such as
`windows_for_workspace_id`, `sort_workspace_windows`, and maybe
`workspace_snapshots(workspace_id, snapshots)`.

Risk: ordering is user-visible in the center window list and in project-label
fallback icon selection. Preserve the current `(column, row, id, path_key)`
ordering exactly and add focused tests before changing consumers.

Verification: unit tests for workspace filtering, stable ordering, and fallback
icon selection.

### P2 - Add a small app action runner

User-triggered app actions repeat thread spawning, one-off Tokio runtime setup,
process execution, and ad hoc error logging.

Evidence:

- PowerProfiles builds a current-thread Tokio runtime on a spawned thread in
  `app/src/widgets/bar/power_profile.rs:45`.
- BlueZ power and connect/disconnect repeat the same runtime setup in
  `app/src/widgets/bar/bluetooth/source.rs:51` and
  `app/src/widgets/bar/bluetooth/source.rs:85`.
- Audio route selection shells out on a bare thread in
  `app/src/widgets/bar/audio/source.rs:32`.
- Notification-center toggle spawns a bare request thread in
  `app/src/widgets/bar/mod.rs:948`.
- MPRIS controls shell out through `playerctl` on a bare thread in
  `app/src/widgets/bar/mod.rs:1060`.
- Theme accent sync shells out synchronously in `app/src/theme.rs:101`.

Target: add a minimal app-local utility such as `app/src/actions.rs` with
`spawn_logged(label, FnOnce -> Result<(), String>)`, `spawn_process`, and
`spawn_system_dbus(label, async move { ... })`. Service modules can then expose
typed actions like `bluez::set_adapter_power` and
`power_profiles::set_active_profile`, while widgets remain thin event
dispatchers.

Risk: do not turn this into a framework command bus or a global source runtime.
The Unix request bridge remains app product behavior. If a shared Tokio runtime
is introduced, it must have clear lifecycle ownership and must not block GTK.

Verification: pure tests for command argument construction where possible; live
manual checks for `scheme-toggle`, notification-center toggle, Bluetooth power,
and power profile cycling.

### P2 - Share XDG path and environment helpers

Path resolution is already repeated across app utilities, and the fallbacks are
not quite centralized.

Evidence:

- Theme setup defines `data_home()` and `config_home()` in
  `app/src/theme.rs:180`, `app/src/theme.rs:184`, and `app/src/theme.rs:191`.
- Material icon fetching defines its own `data_home()` in
  `app/src/widgets/material_icon.rs:169`.
- Desktop icon lookup independently resolves `XDG_DATA_HOME` and
  `XDG_DATA_DIRS` in `app/src/desktop_icon.rs:113`.

Target: add an app-local `app/src/xdg.rs` or `app/src/env_paths.rs` with
`data_home()`, `config_home()`, `data_dirs()`, `icon_theme_dir()`, and
`application_dirs()`. `theme`, `material_icon`, and `desktop_icon` should use
that module.

Risk: path fallback behavior affects live icon and theme discovery. Keep the
current fallback semantics unless intentionally corrected, and test env-var
combinations with temporary environment overrides.

Verification: unit tests for `XDG_DATA_HOME`, `XDG_DATA_DIRS`, `HOME`, and
unset fallbacks.

### P2 - Move reusable popover mounting and row helpers under `widgets`

Generic UI lifecycle code currently lives in the bar module, and list-row
layout patterns are repeated across audio, Bluetooth, and source errors.

Evidence:

- Audio route popover setup allocates a popover, mount box, controller cell,
  and visibility hook in `app/src/widgets/bar/mod.rs:794`.
- The generic lazy mount function lives in the bar module at
  `app/src/widgets/bar/mod.rs:978`, and Bluetooth wraps it at
  `app/src/widgets/bar/mod.rs:1004`.
- Audio route rows manually find and close an ancestor popover before running
  the action in `app/src/widgets/bar/audio/route_row.rs:72`.
- Bluetooth group popover and rows use the same vertical list plus icon/title
  row pattern in `app/src/widgets/bar/bluetooth/mod.rs:77` and
  `app/src/widgets/bar/bluetooth/mod.rs:107`.

Target: add `app/src/widgets/popover.rs` with a lazy component mount helper and
`popdown_ancestor`. Consider a small row/list helper only where it removes
actual repeated GTK layout, not where it hides component-specific behavior.

Risk: Relm4 controller lifetimes are easy to break. Keep row view models and
actions local, and avoid turning simple row components into a hard-to-follow
generic row framework.

Verification: manual UI checks for opening/closing audio routes and each
Bluetooth group popover; confirm controllers are dropped when popovers close.

### P2 - Add an app icon rendering model

Icon semantics are split between material icon fetching, desktop app icon
lookup, and local widget logic that decides whether a string is a theme icon,
app icon, or Material Symbols name.

Evidence:

- Material Symbols resolution and fetch-on-demand live in
  `app/src/widgets/material_icon.rs:21` and
  `app/src/widgets/material_icon.rs:84`.
- Desktop app icon lookup lives separately in `app/src/desktop_icon.rs:7`.
- Project labels switch between app icons and Material icons in
  `app/src/widgets/bar/project_label/mod.rs:233`.
- Window tiles switch agent Material icons against desktop fallback icons in
  `app/src/widgets/bar/window_tile/mod.rs:204`.
- Main bar controls call `material_icon::icon_name(...)` directly at multiple
  call sites, for example `app/src/widgets/bar/mod.rs:186`,
  `app/src/widgets/bar/mod.rs:359`, and `app/src/widgets/bar/mod.rs:667`.

Target: add `app/src/widgets/icons.rs` or expand `material_icon` into an
`icons` module with an `IconRef` / `RenderedIcon` shape, for example Material,
theme symbolic, and app icon variants. Widgets should still choose semantic
icons locally, but rendering code should centralize CSS class and icon-name
conversion.

Risk: material icon lookup has network and filesystem side effects plus a
requested-icon cache. Keep fetch-on-demand behavior centralized and avoid doing
new filesystem or network work in hot render paths. Preserve direct `*-symbolic`
theme icons as pass-through values.

Verification: tests for icon-name conversion and CSS classes; manual check that
Material icons refresh and desktop app icons still resolve.

### P3 - Share tiny text/number helpers only after larger extractions

Small formatting helpers are repeated, but they are lower-value than service
and lifecycle sharing.

Evidence:

- Trim-to-option helpers appear in `app/src/widgets/bar/window_source.rs:48`,
  `app/src/widgets/bar/project_label/source.rs:72`,
  `app/src/widgets/bar/window_tile/source.rs:96`, and
  `app/src/widgets/bar/bzbus/view.rs:450`.
- Percent formatting appears in audio and other widget view helpers, for
  example `app/src/widgets/bar/audio/source.rs:312` and
  `app/src/widgets/bar/mod.rs:1023`.

Target: after the service/module extractions settle, add a tiny app-local
`text` or `format` helper only for clear pure helpers such as
`trimmed_non_empty` or clamped percent formatting.

Risk: domain tooltip wording is product policy and should stay beside each
widget. Premature extraction here would hide context without reducing much
maintenance cost.

Verification: pure unit tests if helpers are extracted.

## Suggested Extraction Order

1. Extract `services::locus_relations`; it removes the most fragile duplicated
   async signal code.
2. Add typed app service modules for Niri, BlueZ, NetworkManager, UPower,
   PowerProfiles, and AgentDBus.
3. Consolidate `bar::window_source` filtering/ordering with tests.
4. Add the app action runner and migrate service actions to it.
5. Centralize XDG paths.
6. Move popover lifecycle helpers under `widgets`.
7. Add icon and tiny text helpers opportunistically when touching those widgets.
