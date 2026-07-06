# Architecture Review: Core Extraction Rerun

Scope: second-pass audit of the current uncommitted tree. This pass focuses on
code that should move into `core/shell-core`, `core/background-effect`,
`core/macros`, `core/rx-macros`, or a core-adjacent service/helper crate, and
calls out code that should explicitly remain in `app`.

Current target state already includes several first-pass extractions:

- `shell_core::source::StateSignal` is now in core and `app/src/hints.rs` only
  owns the product hint state singleton.
- `shell_core::source::switch_map_list` / `switch_map_list_distinct` are now in
  core and the Niri, NetworkManager, and BlueZ list sources use them.
- `shell_core::source::dbus` now owns generic descriptors, ObjectManager model
  streams, cached D-Bus connections for sources, and source error state.
- `shell_core::list` now has an unchanged-list fast path and row reuse by
  `PartialEq`, but it is not yet key-aware.

## Findings

### P1 - Add Observable optional-array D-Bus property helpers

Refs: `core/shell-core/src/source/dbus.rs:285`,
`core/shell-core/src/source/dbus.rs:290`,
`app/src/widgets/bar/niri.rs:32`, `app/src/widgets/bar/niri.rs:75`,
`app/src/widgets/bar/niri.rs:83`, `app/src/widgets/bar/niri.rs:142`,
`app/src/widgets/bar/niri.rs:153`

`shell_core::source::dbus` still exposes the Niri-style optional-array property
conversion only through the old `Source<Option<T>>` path. The current app
Observable path reimplements the same wire conversion in `niri::optional` by
calling `dbus::property::<Vec<T>>(...).map(...)`, then wraps object paths again
in `model_optional`.

Target: `core/shell-core/src/source/dbus.rs`.

API shape:

```rust
pub fn optional_array_property<T>(
    descriptor: PropertyDescriptor,
) -> Observable<Option<T>>
where
    T: Clone + PartialEq + Send + 'static,
    Vec<T>: TryFrom<OwnedValue>,
    <Vec<T> as TryFrom<OwnedValue>>::Error: fmt::Display;

pub fn optional_model_property<T>(
    descriptor: PropertyDescriptor,
) -> Observable<Option<T>>
where
    T: ObjectModel;
```

`optional_model_property` should be a thin map from
`optional_array_property::<OwnedObjectPath>` to `T::at`; if that feels too
model-specific, add only `optional_array_property` and keep the object mapping
in service helpers.

Risks: name the helper after the D-Bus wire encoding. `property<T>()` already
uses `Option<T>` to mean "missing or invalidated property", while this Niri
encoding means "present property carrying zero or one value". Do not use this
as a back door for schema-specific generated path helpers.

### P1 - Add a generic static service descriptor builder

Refs: `core/shell-core/src/source/dbus.rs:60`,
`core/shell-core/src/source/dbus.rs:90`,
`core/shell-core/src/source/dbus.rs:130`,
`app/src/widgets/bar/niri.rs:165`, `app/src/widgets/bar/niri.rs:174`,
`app/src/widgets/bar/bluetooth/model.rs:99`,
`app/src/widgets/bar/bluetooth/model.rs:104`,
`app/src/widgets/bar/network/model.rs:192`,
`app/src/widgets/bar/network/model.rs:205`,
`app/src/widgets/bar/battery.rs:65`,
`app/src/widgets/bar/power_profile.rs:78`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:251`

The app now uses core D-Bus descriptors, but every typed service helper still
repeats the same static descriptor boilerplate: bus name constants,
`ObjectDescriptor::parse(...).expect(...)`, `PropertyDescriptor::new(...)`, and
sometimes ObjectManager descriptor construction. That repetition is generic
descriptor assembly, not widget policy.

Target: `core/shell-core/src/source/dbus.rs`.

API shape:

```rust
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ServiceDescriptor {
    pub bus: Bus,
    pub destination: OwnedBusName,
}

impl ServiceDescriptor {
    pub fn parse(bus: Bus, destination: &str) -> Result<Self, String>;
    pub fn object(&self, path: &str, interface: &str) -> Result<ObjectDescriptor, String>;
    pub fn property(
        &self,
        path: &str,
        interface: &str,
        property: impl Into<String>,
    ) -> Result<PropertyDescriptor, String>;
    pub fn object_manager(&self, path: &str) -> Result<ObjectManagerDescriptor, String>;
}
```

Service-specific modules should still own constants, typed handles, defaults,
and view-model mapping. For example, Niri keeps `NiriWindow::title()`, BlueZ
keeps `BluezDevice::connected()`, and PowerProfiles keeps profile ordering.

Risks: keep this as descriptor construction only. Do not add schema markers,
path extension traits, imperative reads, generated-style relation helpers, or a
provider facade.

### P1 - Move Locus relation watching to the Locus service boundary, not shell-core

Refs: `app/src/widgets/bar/project_label/source/project.rs:12`,
`app/src/widgets/bar/project_label/source/project.rs:17`,
`app/src/widgets/bar/project_label/source/project.rs:52`,
`app/src/widgets/bar/project_label/source/project.rs:120`,
`app/src/widgets/bar/project_label/source/project.rs:148`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:20`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:25`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:60`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:80`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:149`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:173`

Two widget-local providers independently implement the same Locus relation
client: connect to `org.rsynapse.Locus`, subscribe to relation added /
updated / removed / cleared signals, decode the same `RelationRecord`, treat
`ServiceUnknown` as an empty result, and refetch on matching changes.

This is reusable service behavior, but it is not generic shell framework
behavior.

Target: a sibling Locus client/generated-helper crate, or an interim
`app::services::locus_relations` module. Do not move it into `shell-core`.

API shape:

```rust
pub fn relation_targets(subject: String, relation: &'static str) -> Observable<Vec<String>>;
pub fn relation_records(relation: &'static str) -> Observable<Vec<RelationRecord>>;
pub fn first_relation_target(subject: String, relation: &'static str) -> Observable<Option<String>>;
```

The widget providers should keep their display mapping:
`RelationRecord -> ProjectDetails`, `agent-session:* -> AgentSession`, and
`niri-workspace:*` / `niri-window:*` subject construction.

Risks: this crosses into the Locus service contract. If schema/codegen can own
the relation helpers, change that owner and regenerate instead of hand-writing
generated-style APIs here. Avoid recreating the removed provider layer.

### P2 - Add a narrow D-Bus command runner outside `shell_core::source`

Refs: `app/src/widgets/bar/power_profile.rs:45`,
`app/src/widgets/bar/power_profile.rs:48`,
`app/src/widgets/bar/bluetooth/source.rs:51`,
`app/src/widgets/bar/bluetooth/source.rs:58`,
`app/src/widgets/bar/bluetooth/source.rs:85`,
`app/src/widgets/bar/bluetooth/source.rs:89`,
`core/shell-core/src/source/dbus.rs:21`,
`core/shell-core/src/source/dbus.rs:44`,
`core/shell-core/src/source/dbus.rs:52`

PowerProfiles and BlueZ actions repeat the same imperative boilerplate: spawn a
thread, build a current-thread Tokio runtime, connect to the system bus, build
a proxy, then set a property or call a method. Core source code already has
cached D-Bus connection helpers, but they are private to source subscriptions.

Target: `shell_core::dbus_command`, `shell_core::runtime`, or a very small
core-adjacent helper crate. If only `rsynapse-shell` uses it for now, an
app-local `actions` module is a reasonable interim step. Do not put this in
`shell_core::source`.

API shape:

```rust
pub fn spawn_dbus_command<F, Fut>(label: &'static str, bus: dbus::Bus, run: F)
where
    F: FnOnce(zbus::Connection) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static;

pub async fn set_property<T>(
    object: dbus::ObjectDescriptor,
    property: &'static str,
    value: T,
) -> Result<(), String>;

pub async fn call_method<B>(
    object: dbus::ObjectDescriptor,
    method: &'static str,
    body: &B,
) -> Result<(), String>;
```

Typed app functions should remain, such as `bluez::set_adapter_power`,
`bluez::set_device_connected`, and `power_profiles::set_active_profile`.

Risks: this must not become a public one-shot read API, polling layer, command
bus, or source replacement. It should only remove runtime/proxy boilerplate
from GTK event handlers and keep work off the GTK thread.

### P2 - Move lazy popover component hosting to generic GTK/Relm4 support

Refs: `app/src/widgets/bar/mod.rs:794`, `app/src/widgets/bar/mod.rs:803`,
`app/src/widgets/bar/mod.rs:812`, `app/src/widgets/bar/mod.rs:978`,
`app/src/widgets/bar/mod.rs:1004`,
`app/src/widgets/bar/audio/route_row.rs:72`

`mount_popover_component` is generic over `relm4::Component` and only depends
on a `gtk::Popover`, a mount `gtk::Box`, `Controller<C>` lifetime ownership,
and visibility changes. Audio and Bluetooth both use this pattern, and rows
also repeat popover-ancestor lookup to close the menu after an action.

Target: `core/shell-core/src/component/` or `core/shell-core/src/popover.rs`.
An app-local `widgets::popover` helper is acceptable if this should mature
before becoming public core API.

API shape:

```rust
pub struct LazyPopoverComponent<C: relm4::Component> { ... }

impl<C> LazyPopoverComponent<C>
where
    C::Init: Clone + 'static,
    C::Root: AsRef<gtk::Widget> + Clone + Debug,
{
    pub fn attach(popover: &gtk::Popover, init: C::Init) -> Self;
    pub fn refresh(&self);
}

pub fn popdown_ancestor(widget: &impl IsA<gtk::Widget>);
```

Risks: preserve the current lifecycle exactly: launch the child component only
when visible, remove the root widget when hidden, and keep the controller alive
while mounted. Do not encode audio or Bluetooth behavior in the helper.

### P2 - Add keyed list reconciliation and `#[bind_list(..., key = ...)]`

Refs: `core/shell-core/src/list/box_container.rs:49`,
`core/shell-core/src/list/box_container.rs:83`,
`core/macros/src/locus_bindings/view.rs:38`,
`core/macros/src/locus_bindings/view.rs:64`,
`core/macros/src/locus_bindings/view.rs:429`,
`app/src/widgets/bar/mod.rs:163`,
`app/src/widgets/bar/mod.rs:226`,
`app/src/widgets/bar/mod.rs:398`,
`app/src/widgets/bar/mod.rs:689`,
`app/src/widgets/bar/bluetooth/mod.rs:83`

The current list backend now reuses rows by `C::Init: PartialEq`, which is an
improvement over recreating every row. App lists also have stable semantic
keys: Niri paths, source error ids, Bluetooth device paths, and eventual tray
item ids. Keyed reconciliation belongs in core because it is a generic list
binding concern, and macro syntax is the authoring surface.

Target: `core/shell-core/src/list/` plus
`core/macros/src/locus_bindings/view.rs`.

API shape:

```rust
pub struct ComponentListUpdate<'a, C, K = ()> {
    items: &'a [C::Init],
    key: Option<fn(&C::Init) -> K>,
}

impl<'a, C> ComponentListUpdate<'a, C> {
    pub fn new(items: &'a [C::Init]) -> Self;
    pub fn keyed<K>(
        items: &'a [C::Init],
        key: fn(&C::Init) -> K,
    ) -> ComponentListUpdate<'a, C, K>;
}
```

Macro spelling:

```rust
#[bind_list(window_tiles, row = WindowTile, key = WindowNode::path_key)]
```

Risks: keys must be stable and unique within one update. Preserve input order
for GTK children. Do not make key syntax mandatory; simple lists should keep
using the current `PartialEq` path.

### P3 - Consider tiny periodic-source ergonomics only if the pattern grows

Refs: `app/src/widgets/bar/time.rs:17`,
`app/src/widgets/bar/time.rs:18`,
`app/src/widgets/bar/system_stats/source.rs:22`,
`app/src/widgets/bar/system_stats/source.rs:30`

The clock and system stats sources both use
`Shared::<()>::interval(...).start_with(vec![0]).map_err(...)`. This is
acceptable RxRust usage under the Observable-first contract, and there are only
two call sites.

Target, only if more sources repeat this: `shell_core::source`.

API shape:

```rust
pub fn interval(period: Duration) -> Observable<()>;
pub fn periodic<T>(
    period: Duration,
    read: impl Fn() -> Result<T, String> + Send + Sync + 'static,
) -> Observable<T>;
```

Risks: do not use this to hide source ordering bugs or UI lifecycle races. The
`/proc` parsing and clock formatting remain app/widget policy.

## Explicitly Remain In App

### Surface window policy and background blur choices

Refs: `app/src/widgets/bar/mod.rs:962`,
`app/src/widgets/osd/mod.rs:135`,
`app/src/widgets/notifications/mod.rs:124`,
`app/src/widgets/mod.rs:7`,
`core/background-effect/src/lib.rs:12`,
`core/background-effect/src/lib.rs:54`

`WindowConfig` construction for the bar, OSD, and notifications is surface
policy: layer, anchors, namespace, margins, exclusive zone, CSS class names,
and blur radius. `core/background-effect` already owns the generic Wayland
effect machinery and generic region descriptors. Rsynapse class names and
radii should stay in `app`.

Do not add `bar_window`, `osd_window`, notification-center constructors, or
Rsynapse blur presets to core.

### Request CLI/server commands and process routing

Refs: `app/src/request.rs:28`, `app/src/request.rs:126`,
`app/src/request.rs:165`, `app/src/request.rs:272`,
`app/src/request.rs:293`, `app/src/request.rs:302`,
`app/src/request.rs:315`, `app/src/widgets/bar/mod.rs:929`,
`app/src/widgets/notifications/mod.rs:105`

The Unix-socket transport is small and potentially reusable, but the current
file is dominated by product command names and process routing:
`scheme-toggle`, `frost-mode`, `hints`, and notification-center actions. The
roadmap explicitly keeps this bridge in `app` until another consumer needs the
same transport contract.

If reused later, extract only the transport/codec into a core-adjacent crate.
Keep command parsing, usage text, timeouts, request targets, and policies in
`app`.

### Theme, icon, and AGS migration behavior

Refs: `app/src/theme.rs:11`, `app/src/theme.rs:19`,
`app/src/theme.rs:29`, `app/src/theme.rs:101`,
`app/src/desktop_icon.rs:7`, `app/src/desktop_icon.rs:113`,
`app/src/widgets/material_icon.rs:21`,
`app/src/widgets/material_icon.rs:102`

Theme toggling, frost-mode naming, Material icon downloading, desktop-file icon
matching, and the `.config/ags/scripts/sync_accent.sh` bridge are Rsynapse UI
and AGS-migration policy. They should not move to `shell-core`.

Possible app-local sharing still makes sense: an `app::widgets::icons` model
can centralize Material-vs-theme-vs-app icon rendering. That is app sharing,
not core extraction.

### Widget view models and bar ordering/display policy

Refs: `app/src/widgets/bar/window_source.rs:16`,
`app/src/widgets/bar/workspaces.rs:46`,
`app/src/widgets/bar/project_label/source.rs:37`,
`app/src/widgets/bar/project_label/source/workspace_fallback.rs:27`,
`app/src/widgets/bar/project_label/source/agent.rs:15`,
`app/src/widgets/bar/window_tile/source.rs:31`

The bar's `WindowSnapshot`, selected-workspace ordering, project fallback icon
selection, workspace-agent summary, and window-tile kind detection are
Rsynapse display policy. Share repeated filtering/sorting inside
`app/src/widgets/bar/window_source.rs` if needed, but do not move those view
models to core.

`#[shell_macros::observable]` can eventually reduce handwritten derived-source
boilerplate, but the source functions themselves should remain widget/app
functions because they encode display semantics.

### External process and Linux host integrations

Refs: `app/src/widgets/bar/audio/source.rs:32`,
`app/src/widgets/bar/audio/source.rs:41`,
`app/src/widgets/bar/audio/source.rs:116`,
`app/src/widgets/bar/system_stats/source.rs:44`,
`app/src/widgets/bar/mod.rs:1060`,
`app/src/widgets/material_icon.rs:110`

`pw-dump`, `wpctl`, `playerctl`, `/proc/stat`, `/proc/meminfo`, `curl`, and
`gtk-update-icon-cache` integrations are concrete product/service choices. The
general `source::from_task` bridge already exists in core for custom async
loops. Keep parsing and command selection in app unless a dedicated service
client crate becomes a real shared dependency.

## Priority Summary

1. Add Observable optional-array property helpers in `shell_core::source::dbus`.
2. Add a generic D-Bus static service descriptor builder in `shell_core::source::dbus`.
3. Move Locus relation watching to the Locus service boundary or interim app service module.
4. Add a narrow D-Bus command runner outside `shell_core::source`, or an app-local action runner until reused.
5. Move lazy popover component hosting into generic GTK/Relm4 support.
6. Add keyed list reconciliation and macro `key = ...` support.
7. Defer periodic-source ergonomics until more call sites appear.
