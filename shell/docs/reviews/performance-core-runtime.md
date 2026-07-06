# Performance Review: Core Runtime Paths

Scope: `core/shell-core`, `core/background-effect`, `core/macros`, source support, and macro-generated subscription/runtime paths. The review used the current working tree and did not change source code.

## Findings

### High: `shared_by_key` leaks descriptor keys after sources go inactive

Refs: `core/shell-core/src/source/support.rs:137`, `core/shell-core/src/source/support.rs:150`, `core/shell-core/src/source/support.rs:161`, `core/shell-core/src/source/support.rs:459`

`shared_by_key` stores every `SourceKey` in a process-global `HashMap` as `Box<dyn Any>` containing a `Weak<ShareReplayHub<T>>`. Last-subscriber teardown clears the latest value and drops the upstream connection, but it never removes the dead cache entry. Long-lived shells can create unbounded dynamic keys from D-Bus object paths, window paths, project subjects, agent sessions, or per-row derived sources. Even when hubs are dropped, the map retains each descriptor `String`, `TypeId`, and `Weak` forever.

Suggested fix: give `ShareReplayHub` enough identity to remove its own cache entry when the last subscriber drops, or prune dead weak entries on cache miss/insert. Guard removal with pointer equality so a stale hub cannot remove a newer hub for the same key. Add a regression test that creates many unique keys, drops subscriptions, and verifies cache pruning.

### High: generated D-Bus `Source<T>` helpers bypass keyed sharing and replay

Refs: `core/macros/src/dbus_model.rs:74`, `core/macros/src/dbus_model.rs:81`, `core/macros/src/dbus_model.rs:344`, `core/macros/src/dbus_model.rs:371`, `core/shell-core/src/source/dbus.rs:264`, `core/shell-core/src/source/dbus.rs:290`, `core/shell-core/src/source/stream.rs:103`

The `#[dbus_model]` macro emits property methods returning `shell_core::source::Source<T>`. Those methods use `required_property_source` / `optional_property_source`, which create a fresh stream per subscription. That path does not use `shared_by_key`, does not replay latest values to late subscribers, and does not apply `distinct_until_changed`. `property_stream` does share the underlying `PropertiesChanged` signal by object, but duplicate subscribers to the same property still create duplicate property proxies, initial reads, decode work, and model updates.

Suggested fix: converge generated D-Bus model helpers on the Observable path, or add descriptor-keyed sharing to `Source<T>` and use it inside `required_property_source` / `optional_property_source`. Apply `distinct_until_changed` at the descriptor helper boundary where `T: PartialEq`. Prefer retiring the parallel `Source<T>` runtime once the Observable-first API is stable.

### High: D-Bus snapshot sources can miss updates between initial read and signal subscription

Refs: `core/shell-core/src/source/dbus.rs:584`, `core/shell-core/src/source/dbus.rs:590`, `core/shell-core/src/source/dbus.rs:854`, `core/shell-core/src/source/dbus.rs:868`, `core/shell-core/src/source/dbus.rs:952`, `core/shell-core/src/source/dbus.rs:966`

`property_stream`, `object_manager_stream`, and `object_model_stream` perform the initial `Get` / `GetManagedObjects` read before installing the signal stream. A change in the gap between the read and `receive_signal` / `receive_all_signals` is lost, leaving stale UI state until a later update happens. This is a latency and lifecycle issue rather than pure CPU cost: the source can be connected but no longer reflect the service's current state.

Suggested fix: establish the signal stream before the initial read, then emit the initial snapshot. `proxy_property_stream` already follows the safer shape by creating `receive_property_changed` before `get_property`; use that as the model where possible. Add tests with a fake stream/proxy boundary if practical, or at least isolate the state-machine behavior behind smaller units.

### Medium: share/replay broadcasts call observers while holding the hub mutex

Refs: `core/shell-core/src/source/support.rs:249`, `core/shell-core/src/source/support.rs:259`, `core/shell-core/src/source/support.rs:276`, `core/shell-core/src/source/support.rs:290`, `core/shell-core/src/source/support.rs:400`

`ShareReplayState` holds its mutex while replaying, broadcasting next values, and broadcasting terminal events. The generated observers usually just enqueue Relm4 messages, but this code accepts arbitrary Rx observers. A slow observer can block upstream delivery to every subscriber, and a reentrant observer that subscribes/unsubscribes the same keyed source risks lock contention or deadlock.

Suggested fix: avoid invoking observer callbacks under the state lock. One option is to temporarily take or split subscriber storage, perform callbacks outside the lock, then merge surviving subscribers. Another is to delegate fanout to an internal subject designed for callback dispatch. Add a reentrant observer test before refactoring.

### Medium: `Source::from_task` uses unbounded channels for UI streams

Refs: `core/shell-core/src/source/stream.rs:83`, `core/shell-core/src/source/stream.rs:90`, `core/shell-core/src/source/stream.rs:115`, `core/shell-core/src/source/mod.rs:40`

`Source::from_task` creates an unbounded channel. Producers can enqueue faster than the subscription task can poll and forward values, especially for noisy system sources or accidental tight loops. The public Observable bridge `source::from_task` layers `Source::from_task` under `from_stream_result`, so that path can add another task boundary and buffering point.

Suggested fix: use a bounded channel for general task sources, or add explicit latest-value/coalescing helpers for UI state where intermediate values are disposable. Keep truly lossless streams explicit so backpressure behavior is visible at the call site.

### Medium: full-surface background blur retains per-widget layout watchers

Refs: `core/background-effect/src/lib.rs:61`, `core/background-effect/src/effect.rs:147`, `core/background-effect/src/effect.rs:157`, `core/background-effect/src/effect.rs:593`, `core/background-effect/src/effect.rs:621`, `core/background-effect/src/effect.rs:626`

`BackgroundEffectRegion::Surface` reports that it needs layout refresh, which causes `install_dynamic_region_refresh` to build a `LayoutRefresh` over the entire widget tree. Each widget gets width, height, visible, and child-list handlers even though a full-surface blur only depends on the GTK surface size. This retains many signal handlers and requests blur refreshes for unrelated child layout changes.

Suggested fix: split refresh dependencies by region type. `Surface` should track only the window/surface size or frame clock lifecycle. CSS-class regions need widget-tree observation; full-surface regions do not.

### Medium: background-effect region refresh traverses and rebuilds too broadly

Refs: `core/background-effect/src/effect.rs:233`, `core/background-effect/src/effect.rs:255`, `core/background-effect/src/effect.rs:383`, `core/background-effect/src/effect.rs:423`, `core/background-effect/src/effect.rs:452`

On refresh, CSS-class region collection walks the full widget tree. For `BackgroundEffectRegion::Regions`, each nested region recursively performs its own traversal, making refresh cost roughly `regions * widgets`. When child membership changes, `layout_refresh_dirty` causes the next refresh to rebuild the entire watcher tree and reconnect all per-widget signals.

Suggested fix: perform one traversal per refresh and evaluate all requested region descriptors during that pass. For dynamic child changes, consider rebuilding only when the tree shape actually changed and avoid reconnecting watchers for unchanged subtrees. At minimum, add counters or trace fields for watched widget count and traversal count so this cost is visible.

### Medium: `bind_list` reconciliation reparents all rows and searches quadratically

Refs: `core/macros/src/locus_bindings/view.rs:429`, `core/shell-core/src/list/box_container.rs:49`, `core/shell-core/src/list/box_container.rs:56`, `core/shell-core/src/list/box_container.rs:63`, `core/shell-core/src/list/box_container.rs:75`

Macro-generated `#[bind_list]` calls `set_component_list`, whose `gtk::Box` backend removes every existing row from the container, then finds reusable rows with `iter().position()` and `Vec::remove()`. This is O(n^2) for matching plus GTK reparenting/layout churn for every row on every changed list emission, even when most rows are stable.

Suggested fix: skip reconciliation when the item slice is unchanged. For changed lists, build an index of old rows by stable key or by item value, then apply minimal append/move/remove operations. If `PartialEq` remains the only identity contract, use a temporary matched bitmap instead of repeated `Vec::remove`.

### Medium: generated source-model updates mark fields dirty even for equal values

Refs: `core/macros/src/locus_bindings/expand.rs:72`, `core/macros/src/locus_bindings/expand.rs:338`, `core/macros/src/locus_bindings/expand.rs:486`, `core/macros/src/locus_bindings/view.rs:452`

Generated update methods assign every successful emission and mark the field dirty. View setters are track-guarded, but the dirty bit still fires for equal values if the source did not apply `distinct_until_changed`. Several core D-Bus Observable helpers do apply distinctness, but generated `Source<T>` helpers and consumer custom sources are not uniformly protected. This can cascade into expensive setters and `bind_list` reconciliation.

Suggested fix: make descriptor helpers distinct by default where possible. Consider an opt-in model-field equality guard for `T: PartialEq`, or a macro lint/test convention that source expressions used by model fields should be distinct unless repeated values are meaningful.

### Low: background-effect trace mode checks environment variables on every refresh

Refs: `core/background-effect/src/effect.rs:860`, `core/background-effect/src/effect.rs:919`

`TraceMode::from_env()` reads environment variables inside `update_blur_region`, which can run during layout/frame refreshes. The expensive trace formatting is gated correctly, but the env lookup itself is unnecessary repeated work.

Suggested fix: cache `TraceMode` in a `OnceLock`, matching the source/list trace helpers.

## Prioritized Action List

1. Fix `shared_by_key` cache eviction so dynamic descriptors do not leak for the lifetime of the shell.
2. Unify generated D-Bus model property helpers with keyed, replaying Observable semantics, or add equivalent sharing to `Source<T>`.
3. Reorder D-Bus property/ObjectManager setup to subscribe before initial snapshots, closing the stale-state window.
4. Remove observer callbacks from the `ShareReplayHub` mutex critical section and add a reentrancy regression test.
5. Replace `bind_list` full reparenting with a minimal reconciliation path, starting with an unchanged-list fast path.
6. Split background-effect refresh dependencies so full-surface blur does not watch every child widget.
7. Reduce background-effect CSS-region traversal to one pass per refresh and make watcher rebuilds less global.
8. Add bounded or coalescing task-source helpers for noisy UI sources.
9. Enforce distinct source emissions at descriptor or macro boundaries where equal repeated values are not useful.
10. Cache background-effect trace mode.
