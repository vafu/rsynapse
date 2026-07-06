# shell-core Review

Status: reviewed

## Scope

Reusable GTK/Relm4 shell framework crate. Keep product-specific Rsynapse policy
outside this crate.

## Review Order

1. `shell/core/shell-core/Cargo.toml`
2. `shell/core/shell-core/src/lib.rs`
3. `shell/core/shell-core/src/app.rs`
4. `shell/core/shell-core/src/window/*`
5. `shell/core/shell-core/src/css/*`
6. `shell/core/shell-core/src/list/*`
7. `shell/core/shell-core/src/source/mod.rs`
8. `shell/core/shell-core/src/source/stream.rs`
9. `shell/core/shell-core/src/source/state.rs`
10. `shell/core/shell-core/src/source/support.rs`
11. `shell/core/shell-core/src/source/dbus.rs`

## Crate Map

- `src/app.rs` provides the `ShellApp` builder around Relm4 app startup,
  stylesheet loading, optional stylesheet watching, startup hooks, and Relm
  thread configuration.
- `src/window/*` wraps layer-shell window configuration and background effects.
- `src/css/*` loads CSS/SCSS, compiles Sass through an external process, and
  watches style roots for hot reload.
- `src/list/*` reconciles Relm4 row components into GTK boxes for
  macro-generated list bindings.
- `src/source/mod.rs` exports RxRust `Observable` helpers and the newer
  `Source<T>` stream-factory API.
- `src/source/dbus.rs` provides D-Bus descriptors, property/signal streams,
  ObjectManager streams, and typed model helpers.
- `src/source/support.rs` owns shared Observable caching, replay-latest behavior,
  and process-local source error reporting.
- `src/source/state.rs` exposes a mutable process-local `StateSignal<T>`.

## Findings

- High: D-Bus property/ObjectManager streams can miss changes during
  initialization because they perform the initial read before creating the
  signal stream. A property change between `Get` and `PropertiesChanged`
  subscription is lost; likewise an ObjectManager membership change between
  `GetManagedObjects` and `receive_all_signals()` is lost. Relevant code:
  `shell/core/shell-core/src/source/dbus.rs:612`,
  `shell/core/shell-core/src/source/dbus.rs:617`,
  `shell/core/shell-core/src/source/dbus.rs:882`,
  `shell/core/shell-core/src/source/dbus.rs:895`,
  `shell/core/shell-core/src/source/dbus.rs:980`,
  `shell/core/shell-core/src/source/dbus.rs:993`.
- High: replay/shared source delivery calls observers while holding the shared
  replay-state mutex. Reentrant observer behavior, including unsubscribe or
  subscription to the same shared source during `next`, can deadlock. Relevant
  code: `shell/core/shell-core/src/source/support.rs:280`,
  `shell/core/shell-core/src/source/support.rs:290`.
- Medium: `StateSignal` also calls `SharedSubject::next` while holding the
  subject mutex. This has the same reentrancy shape, though the state-value lock
  is released first. Relevant code:
  `shell/core/shell-core/src/source/state.rs:68`,
  `shell/core/shell-core/src/source/state.rs:93`.
- Medium: stylesheet hot reload runs `source.load()` on the GTK main context,
  and SCSS loading shells out to `sass` with blocking `Command::output()`. A
  slow Sass compile can stall the UI during live reload. Relevant code:
  `shell/core/shell-core/src/css/stylesheet.rs:47`,
  `shell/core/shell-core/src/css/compiler.rs:41`.
- Low: `ShellApp::run` and `ShellApp::run_async` duplicate most startup
  plumbing, which makes future stylesheet/startup behavior easy to update in
  one path but not the other. Relevant code:
  `shell/core/shell-core/src/app.rs:110`,
  `shell/core/shell-core/src/app.rs:161`.
- Migration note: both RxRust `Observable<T>` and newer `Source<T>` APIs are
  active. This is manageable while migration is intentional, but new code needs
  clear guidance on which abstraction is preferred for generated bindings and
  hand-written widget sources.

## Refactor Ideas

- For D-Bus sources, install match/signal streams before the initial read, then
  merge the initial snapshot with queued signals, or use zbus APIs that provide
  cached property subscriptions with a known ordering guarantee.
- Rework shared-source fanout to avoid invoking observer callbacks while holding
  internal mutexes. A common pattern is to move/clone the observer list or queue
  deliveries outside the lock, then reconcile removals after callbacks.
- Make `StateSignal` clone the subject handle under lock and call `next` after
  releasing the mutex.
- Move SCSS compilation off the GTK main context during reload, then apply CSS
  back on the main context.
- Extract the shared app-startup body behind `run`/`run_async` so stylesheet and
  startup-hook changes are made once.
- Document the intended `Observable` -> `Source<T>` migration boundary in
  `shell/SOURCE_API.md`.

## Open Questions

- Should `Source<T>` replace RxRust `Observable<T>` for new generated D-Bus
  model helpers, or should it remain an internal bridge until the macro layer is
  fully moved?
- Do D-Bus consumers require strict no-missed-update semantics for properties
  and ObjectManager membership, or is periodic coarse invalidation acceptable
  for the first shell integration?

## Verification

- `cargo test -p shell-core --manifest-path shell/Cargo.toml` passed: 32 tests.
