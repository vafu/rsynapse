# Rsynapse Shell Roadmap

## Core Idea

This repository is the Rsynapse shell UI monorepo. It contains the reusable
Rust shell framework crates plus concrete Rsynapse UI surfaces.

User-facing widgets such as bars, OSDs, notification surfaces, launchers, and
workspace switchers should live outside the framework crates, but inside this
shell monorepo when they are Rsynapse UI components. The framework pieces make
those widgets small, fast, and consistent.

The framework should make this authoring model possible:

```rust
let window = shell_core::window::create_layer_window(config);
```

Consumer crates own:

- Widget role: bar, OSD, notification, launcher, and similar shell surfaces.
- Placement policy: anchors, layer, exclusive zone, surface margins, namespace.
- D-Bus and system source values to subscribe to.
- Rendering and state transitions.
- CSS and visual design.
- Process boundaries, application lifecycle, and whether multiple surfaces live
  in one process or separate binaries.

Framework crates under `core/` own:

- Generic shell app lifecycle setup.
- Global CSS/SCSS registration and development-time stylesheet watching.
- GTK4 / Relm4 integration primitives.
- Layer-shell window creation.
- D-Bus/source subscription plumbing.
- Future macros that reduce Relm4 boilerplate.
- Shared contracts for typed reactive UI state.

## Roadmap

### 1. Foundation: Workspace And Boundaries

- Current framework crates are `core/shell-core`, `core/background-effect`,
  `core/macros`, and `core/rx-macros`.
- `core/background-effect` provides a small reusable GTK4 helper for Wayland
  `ext-background-effect-v1` blur regions. `shell-core` re-exports its config
  enums for `WindowConfig`; external GTK apps can depend on the crate directly
  without taking the rest of shell-core.
- `shell-core` exposes generic framework primitives plus the small Observable
  source facade used by generated code and handwritten sources.
- `shell-macros` subscribes to Observable-compatible source expressions through `shell_core::source`.
- `shell-rx-macros` exposes lightweight declarative macros that expand to
  ordinary RxRust operators for source composition ergonomics.
- `app/` contains the current combined `rsynapse-shell` app for bar, OSD,
  notifications, request bridge, and styles.
- `launcher/` contains the launcher workspace. Package and binary renames such
  as `rsynapse-daemon` to `launcher-daemon` are intentionally deferred.
- Future `bar/`, `osd/`, and `notifications/` crates should be created beside
  `app/` when the combined app is split.
- The old `provider/*` workspace family has been removed. Do not reintroduce Provider, ObservableSource, custom subscription runtime, or D-Bus graph compatibility layers.
- The user-facing source API is Observable-first, described in `SOURCE_API.md`.
- Do not put user-facing bar, OSD, notification, launcher, or workspace switcher behavior in framework crates.

### 2. Shell Core V1

- Add `ShellApp` as the process-level owner for Relm4 app startup, global stylesheets, and long-lived development watchers.
- Finalize `WindowConfig`.
- Keep the API centered on `create_layer_window(config)`.
- Keep naming explicit: `SurfaceMargins`, `Anchors`, `Layer`, `Edge`, `ExclusiveZone`.
- Prefer `ExclusiveZone::Auto` for widgets whose reserved screen area should follow the GTK surface's computed size.
- Document compositor placement versus CSS layout.
- Add pure tests for config behavior.

### 3. Rsynapse UI Consumers

- Use `app/` as the current local consumer for framework ergonomics.
- Keep widget policy, source composition, CSS, and AGS migration behavior in
  UI crates, not in framework crates.
- The `app/` package currently builds two process binaries:
  `rsynapse-shell` for the bar and OSD, and `rsynapse-notifications` for
  notification popups plus the notification center.
- Keep the bar and OSD in the main `rsynapse-shell` binary; do not split OSD
  back into a separate binary unless that consumer policy changes again.
- `app/` owns its Unix-socket request CLI for consumer runtime
  commands such as theme switching, Super-key hints, and notification-center
  control. Notification-center commands route to the `rsynapse-notifications`
  socket; command names and product policy stay out of `shell-core` unless
  another consumer needs the same transport contract.
- Shared product bootstrap such as rsynapse stylesheet/theme setup lives in the
  `app/` library. Only generic app lifecycle setup, including Relm4
  worker-thread configuration, belongs in `shell-core`.

### 4. D-Bus Source Integration

- Use shell-core Observable primitives as the public reactive transport. D-Bus
  clients should be built on `zbus` and hidden behind `shell-core::source`
  primitives or consumer-owned typed helpers.
- Consumer crates should represent D-Bus objects with typed helpers rather than
  raw stringly paths where practical.
- Do not reintroduce schema-specific marker structs, `NodeRef`, `Property`,
  `Relation`, `Path`, or generated graph extension traits in this workspace.
- D-Bus source helpers should return shell-owned `Observable<T>` values.
- Do not add provider/runtime layers back here; use typed services plus
  Observable source functions.
- Avoid FUSE for live shell hot paths.

### 5. Macro Crate

- Keep the `core/macros` crate as the Relm4 source binding proc-macro crate.
- Accept generated typed source expressions instead of raw string tuple paths.
- Integrate directly with `#[relm4::component]` instead of requiring side modules.
- Let consumers declare a typed state model with field-level source attributes:
  - `#[shell_macros::model]`
  - `#[source(...)]`
  - `#[shell_macros::component(model = Bar)]`
- Keep `#[source(...)]` only for model fields. Derived source function
  dependencies use `#[observe(...)]`; stable service dependencies use
  `#[inject]`.
- Add `#[shell_macros::observable]` for user-authored derived source functions
  that return shell-owned `Observable<T>` values.
- Default generated binding modules to `sources`, with `state = ...` available when the component field name needs to be explicit.
- Keep generated runtime internals in one private `__shell` sidecar field on typed models.
- Generate minimal Relm4 glue for source-bound fields:
  - typed model cache
  - typed update messages
  - async watcher startup
- Dispatch binding expressions through Observable sources instead of
  backend-specific watcher functions.
- Generate source messages from result-carrying observable items and keep task
  handles owned by subscriptions.
- Let component views bind GTK setters with `#[bind(field)]`:
  - closure adapters such as `set_label: |title| title.as_str()`
  - function adapters such as `set_css_classes: window_title_classes`
  - generated Relm4 `#[track(...)]` guards so unrelated field changes do not redraw the setter
- Let repeated child regions bind collection fields with `#[bind_list(...)]`.
  The concrete list path is inferred from the annotated widget type. The first
  supported path hosts Relm4 row component controllers on a GTK container;
  GTK-native and Adwaita list adapters should remain optional integrations.
- Keep generated code understandable and debuggable with `cargo expand`.

### 5a. Rx Macro Crate

- Keep `core/rx-macros` as a small declarative macro crate for RxRust operator
  ergonomics.
- Macros in this crate must expand to existing RxRust operators and must not
  introduce source runtimes, subscriptions, watchers, backend clients, or UI
  policy.
- Use it for concise fixed-arity composition such as `combine_latest!` where
  Rust's heterogeneous observable types make `Vec<Observable<_>>` unsuitable.

### 6. Framework Integration Layer

- Connect macro output to source subscriptions.
- Translate source updates, including D-Bus property changes and signals, into
  Relm4 input messages.
- Maintain cached model state for watched GTK properties.
- Avoid client-side polling or a separate reactive runtime.
- Use shared latest observable sources when multiple model fields derive from
  the same upstream descriptor.
- Keep selected object -> dependent collection flows in Observable source
  functions rather than component lifecycle code.
- Prefer semantic source functions such as `selected_workspace_windows()` over
  raw backend traversal at widget call sites.
- Prefer dynamic child components with local source bindings for repeated graph
  items, such as `WindowTitle` taking a `String` node path and binding
  `window_title(window.clone())` internally.
- Let Relm4 components wrap generated source messages in a richer input enum when they need local events, dynamic child rows, or factory messages.
- Support derived observable source functions for summarized UI data, such as
  workspace status, window indicators, build status, agent state, and system
  indicators.

### 7. User-Facing Widgets

- Create actual shell widgets outside the `core/` framework boundary.
- The current consumer package is `app/`, with bar and OSD windows in
  `rsynapse-shell` and notification windows in `rsynapse-notifications`.
- The launcher workspace lives in `launcher/`.
- Consumer crates depend on `shell-core` and `shell-macros`; live desktop data
  should come from D-Bus-backed Observable source functions.

### 8. Hardening

- Add examples and docs once APIs settle.
- Add integration tests where possible.
- Add macro debugging guidance.
- Validate runtime behavior on a real Wayland compositor.

### 9. Observable Source API Migration

- Completed: removed the provider task runtime and `provider/*` crates.
- Completed: replaced `ObservableSource<T>` with the shell-owned `Observable<T>` alias/re-export backed by `rxrust`.
- Completed: removed the filesystem-backed source primitives and active
  widget consumers from this workspace; the old implementation was moved out
  for reference.
- Completed: added generic D-Bus source primitives in `shell_core::source::dbus`
  for properties, signals, and ObjectManager snapshots.
- Keep model `#[source(...)]` bindings as plain value fields and subscribe to
  Observable sources in generated Relm4 glue.
- Add `#[shell_macros::observable]` derived source functions with explicit
  `#[observe(...)]` observable dependencies and `#[inject]` DI service
  dependencies.
- Use `nject` behind a small shell facade for stable services. Reactive graph
  values remain Observable dependencies, not DI services.
- Keep context-dependent source factory behavior in macros/codegen. Shell
  authors should see ordinary Rust functions returning `Observable<T>`, not
  custom source traits.
- Future service helpers should return observables over D-Bus or other
  async-friendly transports.
- Completed: `source::shared_by_key` provides descriptor-keyed sharing for
  handwritten derived sources where reuse is expected, so widget authors do not
  need local `OnceLock` caches or manual `.shared()` calls.
- Completed in `rsynapse-shell`: app-local request CLI/server for
  `scheme-toggle` and `hints active|show|hide|toggle`, matching the useful AGS
  request-handler behavior without adding a framework-level command bus.
- Replace placeholder app surfaces with typed D-Bus-backed widget sources.
- Replace ad hoc consumer source code with user-authored observable source
  functions where it improves ergonomics.

## Next Concrete Step

Continue replacing ad hoc app sources with typed D-Bus-backed helper modules,
starting with the remaining Niri, notification, audio, media, and tray paths
that still need stable service contracts.
