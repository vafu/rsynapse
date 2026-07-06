# Milestone 01: Locus Shell D-Bus Sources

## Goal

Make `locus-shell` the framework-only source and widget foundation for:

```text
D-Bus services -> zbus async streams -> shell_core::source::Observable<T> -> Relm4
```

This milestone adds reusable D-Bus Observable primitives to `shell-core::source`
while keeping `shell-macros` backend-neutral.

## API Shape

Add a `shell_core::source::dbus` module with descriptor-based Observable
constructors:

```rust
source::dbus::property::<T>(descriptor) -> Observable<Option<T>>
source::dbus::property_or::<T>(descriptor, default) -> Observable<T>
source::dbus::signal::<T>(descriptor) -> Observable<T>
source::dbus::object_manager(descriptor) -> Observable<Vec<DbusObject>>
```

Suggested descriptor concepts:

```rust
pub enum Bus {
    Session,
    System,
}

pub struct ObjectDescriptor {
    pub bus: Bus,
    pub destination: zbus::names::OwnedBusName,
    pub path: zbus::zvariant::OwnedObjectPath,
    pub interface: zbus::names::OwnedInterfaceName,
}

pub struct PropertyDescriptor {
    pub object: ObjectDescriptor,
    pub property: String,
}

pub struct SignalDescriptor {
    pub object: ObjectDescriptor,
    pub signal: String,
}
```

Expected behavior:

- Property sources emit an initial read, then `PropertiesChanged` updates.
- Signal sources decode typed payloads.
- ObjectManager sources emit snapshots and then object membership changes.
- Sources share by stable descriptor key and replay the latest value.
- Upstream work starts on first subscriber and stops after the last subscriber
  drops.
- Decode and transport errors use the shell-owned Observable error path.

Macros should continue accepting normal source expressions:

```rust
#[source(upower_battery_percent())]
pub battery_percent: f64,
```

Service-specific helpers live in consumers unless they are truly generic.

## Scope

- Add `zbus`-backed source primitives in `shell-core::source`.
- Keep backend clients private to implementation modules.
- Preserve the existing `Observable<T>` authoring model.
- Add focused tests for descriptors, sharing, cancellation, and D-Bus update
  behavior.
- Prove the API with one `~/proj/rsynapse/shell` source helper.

## Non-Scope

- Reintroducing `provider/*`, `ObservableSource<T>`, graph marker types, or a
  custom provider runtime.
- Moving product behavior back into `locus-shell`.
- Teaching `shell-macros` about D-Bus descriptors or `zbus`.
- Replacing every legacy source in one step.
- Adding public one-shot read helpers or imperative D-Bus clients.

## Implementation Steps

1. Add a minimal `zbus` dependency to `shell-core`.
2. Add private D-Bus source implementation modules.
3. Expose a curated `source::dbus` API from `source/mod.rs`.
4. Reuse `source::shared_by_key` for descriptor-keyed sharing.
5. Implement property sources with initial read plus `PropertiesChanged`.
6. Implement typed signal sources.
7. Implement ObjectManager snapshots and change updates.
8. Add tests for descriptor validation, key stability, replay, cancellation, and
   unrelated signal filtering.
9. Confirm macro-generated subscriptions remain backend-neutral.
10. Migrate one low-risk Rsynapse source helper as a proving consumer.

## Verification

```sh
env CARGO_TARGET_DIR=/tmp/locus-shell-target cargo test -p shell-core source::dbus
env CARGO_TARGET_DIR=/tmp/locus-shell-target cargo test -p shell-core source::support::tests
env CARGO_TARGET_DIR=/tmp/locus-shell-target cargo test -p shell-macros
env CARGO_TARGET_DIR=/tmp/locus-shell-target cargo test --workspace
cargo fmt --check
```

## Risks

- `zbus` runtime assumptions may conflict with current Tokio usage.
- Initial reads and change streams can duplicate or miss values if ordering is
  not deliberate.
- Descriptor keys can accidentally collapse distinct D-Bus members.
- ObjectManager values can become too generic if typed service helpers are not
  introduced soon enough.
- Convenience APIs can leak product policy into `shell-core`.

## Done Criteria

- `shell-core::source::dbus` exposes documented Observable primitives for
  properties, signals, and ObjectManager data.
- D-Bus source work is private, async, cancellable, shared by descriptor, and
  replay-latest.
- `shell-macros` remains backend-neutral.
- At least one external `~/proj/rsynapse/shell` source uses the new D-Bus
  Observable path.
- Focused tests, workspace tests, and formatting pass.
