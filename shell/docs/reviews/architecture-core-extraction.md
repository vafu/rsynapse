# Architecture Review: Core Extraction Candidates

Scope: functions and local utilities in `app/` that look like reusable framework
mechanics for `core/shell-core`, `core/macros`, `core/background-effect`, or a
separate core-adjacent crate. This intentionally excludes Rsynapse display
policy, concrete widget view models, and product command names.

## Findings

### P1 - Add a generic observable state cell to `shell_core::source`

Refs: `app/src/hints.rs:13`, `app/src/hints.rs:28`, `app/src/hints.rs:48`,
`app/src/hints.rs:53`, `app/src/hints.rs:74`

`hints_active()` hand-rolls a replaying mutable source with `OnceLock`,
`Mutex<bool>`, `SharedSubject`, and a custom `ObservableType` implementation.
The behavior is not hint-specific: hold a latest local value, emit the current
value on subscribe, and broadcast changes from imperative app events.

Target: `core/shell-core/src/source/`, probably `source::state`.

Proposed API shape:

```rust
pub struct StateSignal<T> { ... }

impl<T> StateSignal<T>
where
    T: Clone + PartialEq + Send + 'static,
{
    pub fn new(initial: T) -> Self;
    pub fn observable(&self) -> Observable<T>;
    pub fn get(&self) -> T;
    pub fn set(&self, value: T);
    pub fn update(&self, f: impl FnOnce(&mut T));
}
```

Use app-local `OnceLock<StateSignal<bool>>` only to own product state; keep the
subject/replay mechanics in core. This would also be useful for future local UI
state such as notification center visibility when it needs source bindings.

Risks: avoid turning this into a global store or provider facade. It should be a
small process-local primitive with `shared_by_key`-compatible semantics:
replay latest, avoid duplicate emissions for equal values, and avoid invoking
observers while holding a mutex.

### P1 - Add object-list snapshot combinators to `shell_core::source`

Refs: `core/shell-core/src/source/mod.rs:53`, `core/shell-core/src/source/mod.rs:97`,
`app/src/widgets/bar/bluetooth/source.rs:14`, `app/src/widgets/bar/network/model.rs:118`,
`app/src/widgets/bar/window_source.rs:16`, `app/src/widgets/bar/workspaces.rs:14`,
`app/src/widgets/bar/project_label/source/agent.rs:15`

Several app sources repeat the same higher-order pattern:

1. subscribe to an observable list of handles,
2. map each handle to an observable snapshot,
3. combine the current per-item snapshots into `Vec<_>`,
4. cancel the old per-item graph when the handle list changes.

The current spellings use `switch_map` plus `combine_latest_vec` at every call
site. Sorting, filtering, grouping, and display mapping should stay in app, but
the "observable list to latest snapshot list" composition is generic.

Target: `core/shell-core/src/source/mod.rs`.

Proposed API shape:

```rust
pub fn switch_map_list<T, U>(
    items: Observable<Vec<T>>,
    map: impl Fn(T) -> Observable<U> + Send + Sync + 'static,
) -> Observable<Vec<U>>
where
    T: Send + 'static,
    U: Clone + Send + 'static;

pub fn switch_map_list_distinct<T, U>(...) -> Observable<Vec<U>>
where
    U: Clone + PartialEq + Send + 'static;
```

Example replacement: `network_devices()` could become
`source::switch_map_list(dbus::models::<NetworkDevice>(network_manager()), device_snapshot)`
instead of open-coding the switch/combine sequence.

Risks: empty-list behavior must remain `Vec::new()` immediately, matching
`combine_latest_vec`. The helper should preserve input order and should not
invent identity, sorting, deduping, or widget lifecycle behavior. It also
depends on the `shared_by_key` leak and replay issues noted in
`docs/reviews/performance-core-runtime.md`.

### P1 - Expose Observable optional-array D-Bus property helpers

Refs: `core/shell-core/src/source/dbus.rs:231`, `core/shell-core/src/source/dbus.rs:290`,
`app/src/widgets/bar/niri.rs:32`, `app/src/widgets/bar/niri.rs:83`,
`app/src/widgets/bar/niri.rs:142`, `app/src/widgets/bar/niri.rs:153`

`shell_core::source::dbus` already has `optional_property_source<T>` for the
generated `Source<T>` path, because Niri-style optional properties are encoded
as arrays with zero or one item. The hand-written Niri service helper reimplements
the same conversion on the Observable path with `property::<Vec<T>>(...).map(...)`.

Target: `core/shell-core/src/source/dbus.rs`.

Proposed API shape:

```rust
pub fn optional_array_property<T>(
    descriptor: PropertyDescriptor,
) -> Observable<Option<T>>
where
    T: Clone + PartialEq + Send + 'static,
    Vec<T>: TryFrom<OwnedValue>,
    <Vec<T> as TryFrom<OwnedValue>>::Error: fmt::Display;

pub fn optional_object_model<T>(
    descriptor: PropertyDescriptor,
) -> Observable<Option<T>>
where
    T: ObjectModel;
```

`optional_object_model` can be a thin mapping from
`optional_array_property::<OwnedObjectPath>` to `T::at`. If that is too
schema-flavored, keep only the array helper and let service helpers map object
paths themselves.

Risks: name the helper after the wire encoding, not after "optional" generally,
so normal absent D-Bus properties still use `property<T>() -> Observable<Option<T>>`.
Do not reintroduce generated graph path traits or marker structs in this repo.

### P1 - Move repeated Locus relation watching to the Locus service boundary

Refs: `app/src/widgets/bar/project_label/source/project.rs:12`,
`app/src/widgets/bar/project_label/source/project.rs:52`,
`app/src/widgets/bar/project_label/source/project.rs:120`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:20`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:60`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:80`,
`app/src/widgets/bar/window_tile/agent/source/actual.rs:149`

Two widget providers independently connect to `org.rsynapse.Locus`,
subscribe to `RelationAdded`, `RelationUpdated`, `RelationRemoved`, and
`RelationCleared`, decode the same relation DTO, handle `ServiceUnknown` as an
empty result, and refetch on matching changes. This is reusable service-client
behavior, but it is not generic shell-core behavior.

Target: a sibling Locus client/service crate, or generated helpers from
`../locus`, consumed by `app`. If that is not ready, use an app-local
service module as an interim step; do not put this in `shell-core`.

Proposed API shape:

```rust
pub fn relation_targets(subject: String, relation: &'static str) -> Observable<Vec<String>>;

pub fn relation_records(relation: &'static str) -> Observable<Vec<RelationRecord>>;

pub fn first_relation_record(
    subject: String,
    relation: &'static str,
) -> Observable<Option<RelationRecord>>;
```

Widget code should keep its display mapping: project metadata to
`ProjectDetails`, agent target to `AgentSession`, and window/workspace subject
construction.

Risks: this touches a sibling service contract. Avoid hand-writing generated
schema APIs in this repo; if Locus schema/codegen can express this, change that
owner instead. Also avoid recreating the old provider facade.

### P2 - Add generic D-Bus command helpers outside `shell_core::source`

Refs: `app/src/widgets/bar/power_profile.rs:45`,
`app/src/widgets/bar/bluetooth/source.rs:55`,
`app/src/widgets/bar/bluetooth/source.rs:89`,
`core/shell-core/src/source/dbus.rs:21`, `core/shell-core/src/source/dbus.rs:197`

Several widget actions repeat the same imperative D-Bus boilerplate: spawn a
thread, build a current-thread Tokio runtime, connect to the system bus, create
a proxy, then set a property or call a method. The product decisions stay in
app, but the runtime/proxy boilerplate is generic.

Target: `shell_core::dbus` or `shell_core::runtime`, not
`shell_core::source`. The AGENTS guidance specifically warns against public
one-shot read helpers or imperative clients in `shell-core::source`; this should
be a command helper for UI event handlers, not a source primitive.

Proposed API shape:

```rust
pub fn spawn_dbus_command(
    label: &'static str,
    bus: dbus::Bus,
    run: impl FnOnce(zbus::Connection) -> Fut + Send + 'static,
);

pub async fn set_property<T>(
    object: dbus::ObjectDescriptor,
    property: &'static str,
    value: T,
) -> Result<(), String>
where
    T: zbus::zvariant::Type + serde::Serialize + Send + Sync;

pub async fn call_method<B>(
    object: dbus::ObjectDescriptor,
    method: &'static str,
    body: &B,
) -> Result<(), String>
where
    B: zbus::zvariant::Type + serde::Serialize + Sync;
```

App service modules would still expose typed actions such as
`bluez::set_adapter_power`, `bluez::set_device_connected`, and
`power_profiles::set_active_profile`.

Risks: this can easily become an imperative D-Bus client layer. Keep it narrow:
no read API, no polling, no subscriptions, no product command names, and no
replacement for Observable sources. It should reuse cached zbus connections if
possible, but must not block GTK.

### P2 - Add a lazy popover component host utility

Refs: `core/shell-core/src/list/mod.rs:21`,
`app/src/widgets/bar/mod.rs:794`, `app/src/widgets/bar/mod.rs:978`,
`app/src/widgets/bar/mod.rs:1004`, `app/src/widgets/bar/audio/route_row.rs:72`

`mount_popover_component` is generic over `relm4::Component` and only depends
on a `gtk::Popover`, a mount `gtk::Box`, and a stored `Controller<C>`. The bar
uses it for the audio route popover and Bluetooth group popovers. This is close
to `shell_core::list` in spirit: generic GTK/Relm4 component lifecycle glue for
widgets.

Target: `core/shell-core/src/component/` or `core/shell-core/src/list/` if the
API is framed as a list/popover backend. A less aggressive interim target is
`app::widgets`, but the helper itself has no Rsynapse policy.

Proposed API shape:

```rust
pub struct LazyPopoverComponent<C: relm4::Component> { ... }

impl<C> LazyPopoverComponent<C>
where
    C::Init: Clone,
    C::Root: AsRef<gtk::Widget> + Clone + Debug,
{
    pub fn attach(popover: &gtk::Popover, init: C::Init) -> Self;
    pub fn mount_if_visible(&self);
}

pub fn popdown_ancestor(widget: &impl IsA<gtk::Widget>);
```

The helper should own the mount box, visibility-notify hook, and controller
lifetime. Row actions can call `popdown_ancestor` instead of repeating ancestry
lookup.

Risks: component controller lifetime bugs are easy to introduce. Preserve the
current behavior exactly: launch only while visible, remove the root widget when
hidden, and keep init cloning explicit. This should not encode Bluetooth or
audio-specific policy.

### P2 - Consider keyed list reconciliation support in `shell_core::list`

Refs: `core/macros/src/locus_bindings/view.rs:38`,
`core/macros/src/locus_bindings/view.rs:429`,
`core/shell-core/src/list/mod.rs:5`, `core/shell-core/src/list/box_container.rs:49`,
`app/src/widgets/bar/mod.rs:163`, `app/src/widgets/bar/mod.rs:226`,
`app/src/widgets/bar/mod.rs:689`, `app/src/widgets/bar/bluetooth/mod.rs:83`

`#[bind_list]` currently passes only a slice of row init values into
`ComponentListUpdate`, and the `gtk::Box` backend matches rows by `PartialEq`.
App lists now include windows, workspaces, Bluetooth devices, and source errors;
these all have stable semantic keys. A generic key-aware list update belongs in
core rather than app-specific row code.

Target: `core/shell-core/src/list/` plus `core/macros/src/locus_bindings/view.rs`
for macro syntax.

Proposed API shape:

```rust
pub struct ComponentListUpdate<'a, C, K = ()> {
    items: &'a [C::Init],
    key: Option<fn(&C::Init) -> K>,
}

impl<'a, C> ComponentListUpdate<'a, C> {
    pub fn new(items: &'a [C::Init]) -> Self;
    pub fn keyed<K>(items: &'a [C::Init], key: fn(&C::Init) -> K) -> ComponentListUpdate<'a, C, K>;
}
```

Macro spelling could be `#[bind_list(window_tiles, row = WindowTile, key =
WindowNode::path_key)]` once needed.

Risks: this overlaps with performance work already noted in
`docs/reviews/performance-core-runtime.md`. Do the unchanged-list fast path and
minimal reconciliation first; add public key syntax only when the existing
`PartialEq` identity is not enough.

### P3 - Defer generic Unix request transport extraction

Refs: `app/src/request.rs:28`, `app/src/request.rs:73`,
`app/src/request.rs:105`, `app/src/request.rs:127`,
`app/src/request.rs:236`, `app/src/request.rs:272`,
`app/src/request.rs:352`, `app/src/request.rs:379`,
`app/src/widgets/bar/mod.rs:746`, `app/src/widgets/notifications/mod.rs:77`

The request bridge mixes two layers: product commands such as `scheme-toggle`,
`frost-mode`, `hints`, and notification-center actions, plus a small NUL-arg
Unix socket request/response transport. The transport is reusable, but the
project docs explicitly keep this bridge in app unless another consumer needs
the same contract.

Target if reuse appears: a small separate core-adjacent crate such as
`core/local-request`, not `shell-core::source`.

Proposed API shape:

```rust
pub trait RequestCodec {
    type Request;
    type Response;

    fn decode_request(args: &[String]) -> Result<Self::Request, String>;
    fn encode_response(response: &Self::Response) -> String;
    fn decode_response(line: &str) -> Result<Self::Response, String>;
}

pub struct LocalRequestServer<C: RequestCodec> { ... }
pub fn send<C: RequestCodec>(socket: &Path, args: &[String]) -> Result<C::Response, String>;
```

Risks: extracting this now would fight the roadmap. Keep command names,
target routing, timeouts, CLI usage text, and product policies in `app`.

### P3 - Keep layer/window configs in app; add only tiny core ergonomics later

Refs: `core/shell-core/src/window/config.rs:59`,
`core/shell-core/src/window/layer.rs:11`,
`app/src/widgets/bar/mod.rs:962`, `app/src/widgets/osd/mod.rs:135`,
`app/src/widgets/notifications/mod.rs:124`

The bar, OSD, and notification-center `WindowConfig` constructors are
product/surface policy: layer, anchors, namespace, margins, exclusive zone, and
background blur radius differ by surface. They should stay in app.

Potential core ergonomics, if repeated more widely:

```rust
impl Anchors {
    pub const fn horizontal(edge: Edge) -> Self; // bottom+left+right style helper
    pub const fn corner(vertical: Edge, horizontal: Edge) -> Self;
}
```

Target: `core/shell-core/src/window/config.rs`, only for generic anchor/margin
builders.

Risks: do not add `bar_window`, `osd_window`, notification placement helpers,
or shell role constructors to core. Those are explicitly consumer decisions in
`PROJECT.md` and `PLAN.md`.

## Priority Summary

1. Add `source::StateSignal` or equivalent replaying mutable local state.
2. Add `source::switch_map_list` / latest snapshot-list combinator.
3. Expose Observable optional-array D-Bus property helpers.
4. Move Locus relation watching to the Locus service boundary, not shell-core.
5. Add narrow D-Bus command helpers outside `shell_core::source`.
6. Add a lazy popover component host utility.
7. Evolve `shell_core::list` toward keyed reconciliation after the performance fixes.
8. Defer Unix request transport extraction until another consumer needs it.
9. Keep current surface-specific window configs in app.
