# Performance Review: Core Runtime Rerun

Scope: second-pass performance audit of `core/shell-core`, `core/background-effect`, `core/macros`, source support, and macro-generated subscription/runtime paths. This review treats the current uncommitted working tree as the target state and does not assess historical commits.

## Current-State Notes

- The prior `shared_by_key` dead-key leak is partly addressed: `shared_with_key` now prunes inactive weak cache entries before lookup/insert (`core/shell-core/src/source/support.rs:137`). That fixes the unbounded dead-entry retention pattern from the first pass, but it introduces a new cache-scan cost noted below.
- The prior background-effect trace env lookup issue is addressed: `TraceMode::from_env()` now uses `OnceLock` (`core/background-effect/src/effect.rs:920`).
- `set_component_list` now has an unchanged-list fast path (`core/shell-core/src/list/box_container.rs:54`), which reduces redundant work for equal repeated emissions. Changed-list reconciliation is still expensive.

## Findings

### High: Generated D-Bus `Source<T>` helpers still bypass keyed replay/distinctness

Refs: `core/macros/src/dbus_model.rs:81`, `core/macros/src/dbus_model.rs:344`, `core/macros/src/dbus_model.rs:369`, `core/shell-core/src/source/dbus.rs:264`, `core/shell-core/src/source/dbus.rs:290`, `core/shell-core/src/source/stream.rs:9`, `core/shell-core/src/source/stream.rs:83`, `core/macros/src/view_model.rs:244`

`#[dbus_model]` still generates property methods returning `shell_core::source::Source<T>`, and those methods route through `required_property_source` / `optional_property_source`. This path creates a fresh stream/task per subscription, uses the `Source<T>` compatibility runtime, and does not get `shared_by_key` replay or descriptor-level `distinct_until_changed`. The Observable `dbus::property` helper is keyed and distinct, but generated models do not use that path.

Rationale: repeated rows or nested view models that ask for the same object property can multiply property proxies, initial reads, decode work, channel buffers, and Relm4 messages. This also keeps a second subscription runtime alive in the core API, which makes cancellation and backpressure behavior less predictable.

Suggested fix: converge generated D-Bus models on Observable-returning helpers, or add keyed share/replay/distinct behavior inside the `Source<T>` property helpers as a temporary bridge. Prefer making the generated method return `Observable<T>` / `Observable<Option<T>>` so field-level `#[source(...)]` and D-Bus model helpers share one runtime contract.

### High: D-Bus snapshot streams can miss updates during startup

Refs: `core/shell-core/src/source/dbus.rs:584`, `core/shell-core/src/source/dbus.rs:590`, `core/shell-core/src/source/dbus.rs:854`, `core/shell-core/src/source/dbus.rs:868`, `core/shell-core/src/source/dbus.rs:952`, `core/shell-core/src/source/dbus.rs:966`

`property_stream`, `object_manager_stream`, and `object_model_stream` perform the initial `Get` / `GetManagedObjects` read before installing the signal stream. A service update between the initial read and the later `properties_changed(...).into_stream()` or `receive_all_signals()` setup is lost.

Rationale: this is a latency and correctness issue in the runtime hot path. The UI can become connected but stale until a later unrelated signal arrives, which is especially visible for fast object churn such as windows, tray items, or notification membership.

Suggested fix: subscribe to the signal stream before the initial snapshot, then emit the snapshot and fold queued signals after it. `proxy_property_stream` already uses the safer sequence by creating `receive_property_changed` before `get_property`; use that shape where possible.

### Medium: Share/replay invokes observers while holding the hub mutex

Refs: `core/shell-core/src/source/support.rs:271`, `core/shell-core/src/source/support.rs:281`, `core/shell-core/src/source/support.rs:298`, `core/shell-core/src/source/support.rs:312`, `core/shell-core/src/source/support.rs:425`, `core/shell-core/src/source/support.rs:431`

`ShareReplayState` calls `observer.next`, `observer.error`, and `observer.complete` while the `ShareReplayState` mutex is held. Error and complete paths `take` the observer list, but the mutex guard is still active while callbacks run.

Rationale: generated observers usually enqueue Relm4 messages, but this is a public Observable primitive and can accept arbitrary observers. A slow observer stalls every subscriber and upstream delivery. A reentrant observer that subscribes/unsubscribes the same keyed source can contend on, or deadlock against, the hub mutex.

Suggested fix: move callback dispatch outside the state lock. Capture the target observer ids and cloned payloads while locked, release the lock, then invoke callbacks. For terminal events, take the observers under lock and drop the guard before dispatch. Add a regression test with a reentrant observer.

### Medium: `StateSignal` and source-error replay have subscribe-time lost-update windows

Refs: `core/shell-core/src/source/state.rs:53`, `core/shell-core/src/source/state.rs:68`, `core/shell-core/src/source/state.rs:126`, `core/shell-core/src/source/state.rs:135`, `core/shell-core/src/source/support.rs:547`, `core/shell-core/src/source/support.rs:550`, `core/shell-core/src/source/support.rs:576`, `core/shell-core/src/source/support.rs:596`

`StateSignalObservable::subscribe` reads and emits the current value, then subscribes to the subject. `SourceErrorSnapshots` does the same for error state. A concurrent `set` / `record_source_error` between the snapshot emission and subject subscription is missed by the new subscriber.

Rationale: these are replaying state primitives, so a subscriber should not be able to start behind the latest state. Missing a state transition can leave UI stale until another update happens.

Suggested fix: use one lock for value plus subscriber registration, or replace the raw subject with the same share/replay machinery after its callback locking issue is fixed. At minimum, add tests that force an update between snapshot and subscription registration.

### Medium: `shared_by_key` now prunes with an O(cache) global scan on every lookup

Refs: `core/shell-core/src/source/support.rs:137`, `core/shell-core/src/source/support.rs:139`, `core/shell-core/src/source/support.rs:148`, `core/shell-core/src/source/support.rs:150`

The cache leak fix calls `cache.retain(|_, cached| cached.is_alive())` for every `shared_by_key` construction. This keeps memory bounded, but every source construction now scans all cached descriptors under the global cache mutex. Hits also build trace labels eagerly before `trace_source_lifecycle` checks whether tracing is enabled.

Rationale: source construction is not per-frame, but it happens during window/list churn and nested row creation. A long-lived shell with many live D-Bus descriptors can turn each new source into a global linear scan and serialization point.

Suggested fix: remove entries on hub drop/last unsubscribe with pointer-equality protection, or prune opportunistically only after misses, thresholds, or observed dead entries. Make lifecycle trace helpers take `fmt::Arguments` or closures so labels are not formatted when tracing is disabled.

### Medium: `Source::from_task` and its Observable bridge use unbounded channels

Refs: `core/shell-core/src/source/stream.rs:78`, `core/shell-core/src/source/stream.rs:83`, `core/shell-core/src/source/stream.rs:90`, `core/shell-core/src/source/stream.rs:115`, `core/shell-core/src/source/mod.rs:42`, `core/shell-core/src/source/mod.rs:48`, `core/shell-core/src/source/stream.rs:236`

`Source::from_task` creates an unbounded async channel. The public Observable-era `source::from_task` still wraps this compatibility path, and `Source::switch_map` also uses it internally.

Rationale: noisy producers can enqueue faster than the subscription task forwards to observers, especially when an observer path is slowed by GTK/main-thread work. This can turn transient system churn into heap growth and delayed UI state.

Suggested fix: add bounded or latest-value/coalescing task-source helpers and use those for UI state where intermediate values are disposable. Keep lossless unbounded behavior explicit at the call site.

### Medium: Changed-list reconciliation still removes/reappends all rows and searches quadratically

Refs: `core/macros/src/locus_bindings/view.rs:429`, `core/shell-core/src/list/box_container.rs:49`, `core/shell-core/src/list/box_container.rs:76`, `core/shell-core/src/list/box_container.rs:83`, `core/shell-core/src/list/box_container.rs:95`

The unchanged-list fast path is good, but any changed list still removes every existing widget from the `gtk::Box`, then finds reusable rows with `iter().position()` and `Vec::remove()`.

Rationale: one insertion, removal, or reorder causes O(n^2) matching plus GTK reparent/layout churn for every row. This can dominate source update latency for window/workspace/tray-style lists where most rows remain stable.

Suggested fix: reconcile by stable key when available. If `PartialEq` remains the only identity contract, use a matched bitmap or a temporary index to avoid repeated `Vec::remove`, and apply minimal append/move/remove operations instead of clearing the whole container.

### Medium: Generated updates still dirty fields for equal values

Refs: `core/macros/src/locus_bindings/expand.rs:338`, `core/macros/src/locus_bindings/expand.rs:340`, `core/macros/src/locus_bindings/expand.rs:341`, `core/macros/src/locus_bindings/expand.rs:486`, `core/macros/src/locus_bindings/view.rs:452`, `core/shell-core/src/list/box_container.rs:54`

Generated source-model updates assign every successful emission and mark the field dirty. The new list unchanged fast path reduces one downstream cost, but all tracked setters and nested models still wake for equal values unless the source itself applied distinctness.

Rationale: core D-Bus Observable helpers usually call `distinct_until_changed`, but generated `Source<T>` helpers and custom sources are not uniformly distinct. Equal repeated values can still produce Relm4 messages and run setter/list guards.

Suggested fix: make generated D-Bus helpers distinct by default and consider a field-level equality guard for source-bound model fields where `T: PartialEq`. If macro-level guarding is too invasive, document and test the expectation that model-bound sources should be distinct unless repeated values are semantically meaningful.

### Medium: Full-surface background blur still installs whole-widget-tree layout watchers

Refs: `core/background-effect/src/lib.rs:61`, `core/background-effect/src/lib.rs:67`, `core/background-effect/src/effect.rs:148`, `core/background-effect/src/effect.rs:158`, `core/background-effect/src/effect.rs:595`, `core/background-effect/src/effect.rs:622`, `core/background-effect/src/effect.rs:627`

`BackgroundEffectRegion::Surface` still reports `needs_layout_refresh() == true`, which makes `install_dynamic_region_refresh` build a `LayoutRefresh` over the full widget tree. CSS-class membership watches are skipped for surface regions, but width, height, visible, and child-list watchers are still attached to every descendant.

Rationale: full-surface blur only needs the surface/window size to settle after map/configure. Watching every child keeps many signal handlers alive and queues blur refreshes for child changes that cannot affect a full-surface region.

Suggested fix: split refresh dependency modes. Surface blur should use only frame-clock/surface-size refresh until the surface is stable. CSS-class regions should keep widget-tree observation.

### Medium: CSS-class background regions traverse the widget tree once per region descriptor

Refs: `core/background-effect/src/effect.rs:283`, `core/background-effect/src/effect.rs:302`, `core/background-effect/src/effect.rs:320`, `core/background-effect/src/effect.rs:384`, `core/background-effect/src/effect.rs:400`, `core/background-effect/src/effect.rs:424`, `core/background-effect/src/effect.rs:256`

`BackgroundEffectRegion::Regions` recursively processes each region. Each CSS-class region calls `collect_blur_region_rectangles_for_css_classes`, which starts a full tree walk from the window root. When child membership changes, `layout_refresh_dirty` rebuilds the entire watcher tree on the next refresh.

Rationale: refresh cost is roughly `regions * widgets`, plus watcher reconnect churn on tree changes. Rounded regions also generate many one-pixel bands, so repeated traversals compound geometry work.

Suggested fix: flatten region descriptors and do one widget traversal per refresh, evaluating all CSS-class predicates during that pass. Rebuild layout watchers only for changed subtrees if possible, or at least expose trace counters for watched widget count and traversal count.

## Prioritized Action List

1. Move generated D-Bus model helpers off the `Source<T>` compatibility runtime or add keyed share/replay/distinct behavior there.
2. Reorder D-Bus property/ObjectManager stream setup so signal streams are installed before initial snapshots.
3. Stop invoking share/replay observers while holding the hub mutex.
4. Close replay subscribe races in `StateSignal` and source-error snapshots.
5. Replace `shared_by_key` per-lookup full-cache pruning with drop/threshold-based pruning.
6. Add bounded/latest-value task-source helpers and retire unbounded channels from UI-state bridges.
7. Improve changed-list reconciliation to avoid full reparenting and O(n^2) matching.
8. Add equality guarding or stronger distinctness conventions for generated model updates.
9. Split background-effect refresh modes so surface blur does not watch every descendant.
10. Collapse CSS-class background-effect region collection to one traversal per refresh.
