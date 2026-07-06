# niri-dbus Review

Status: reviewed

## Scope

Session D-Bus projection for niri IPC. It should expose niri outputs,
workspaces, windows, focus, and events without shell UI policy.

## Review Order

1. `niri-dbus/README.md`
2. `niri-dbus/Cargo.toml`
3. `niri-dbus/src/main.rs`
4. `niri-dbus/src/paths.rs`
5. `niri-dbus/src/ipc.rs`
6. `niri-dbus/src/state.rs`
7. `niri-dbus/src/dbus.rs`
8. `niri-dbus/src/service.rs`

## Crate Map

- `src/main.rs` initializes tracing and delegates to the service runner.
- `src/paths.rs` defines bus/interface/path constants and object path encoding.
- `src/ipc.rs` wraps the niri Unix socket, request/response IPC, and event
  stream setup.
- `src/state.rs` owns the in-memory projection over `niri_ipc` state and object
  delta calculation.
- `src/dbus.rs` exposes root, output, workspace, and window properties over
  D-Bus.
- `src/service.rs` owns connection setup, ObjectManager registration, reconnect
  behavior, and property-change emission.

## User Notes

- Use subagents as decision arbiters for file-level review decisions.
- Accept a narrow stable contract/client API for `niri-dbus`, similar to the
  `locus` direction, while keeping daemon internals private.

## Arbiter Decisions

- Add a small `src/lib.rs` contract surface before consumers depend on paths.
  Re-export service/interface/path constants plus `output_path`,
  `workspace_path`, and `window_path`. Do not expose `optional_path` or raw IPC
  wrappers.
- Keep V1 strictly read-only. Command/control methods remain out of scope.
- Keep `org.rsynapse.Niri` as the service name for now.
- D-Bus object paths are acceptable live object references only. Durable Locus
  relations should prefer typed stable keys:
  `org.rsynapse.niri.output.name`, `org.rsynapse.niri.workspace.id`,
  `org.rsynapse.niri.workspace.name`, and live-scoped
  `org.rsynapse.niri.window.id`.
- Replace output path encoding before stabilizing `output_path`; the current
  `_XX` style can collide with literal underscore sequences and empty names.
- Keep separate sockets for `initial_snapshot()` and `event_stream()` for V1.
  niri's event stream provides the current event-state snapshot; version/output
  details remain startup/reconnect snapshots.
- Map EOF during request/response IPC to `UnexpectedEof`.
- Treat IPC wrappers as daemon-private. Retry/backoff belongs in `service.rs`.
- Increment `generation` on meaningful disconnects that clear projected state,
  but not on no-op failed retry loops when already disconnected and empty.
- Keep object-set deltas plus coarse property emission for V1. Fine-grained
  property deltas are a later performance/stabilization item.
- Keep `catch_unwind` as a guard around niri's reducer, but treat a panic as
  stream desync and reconnect rather than continuing the same stream.
- Mutate ObjectServer registration sets only after `at`/`remove` succeeds. On
  failure, fail/restart or reconcile rather than letting local bookkeeping drift.
- Zero-or-one arrays are acceptable on the D-Bus wire for optional properties,
  but stable Rust client APIs should expose `Option<T>`.
- No shell-policy leaks were found in `dbus.rs` or `service.rs`.

## Findings

- Medium: every accepted niri event emits root property changes and then every
  property on every registered output, workspace, and window, regardless of what
  changed. This is simple and correct enough for initial bring-up, but it can
  become noisy under focus/window-layout churn and pushes filtering work onto
  shell consumers. Relevant code: `niri-dbus/src/service.rs:81`,
  `niri-dbus/src/service.rs:150`, `niri-dbus/src/service.rs:176`.
- Medium: object registration bookkeeping mutates `registered_*` before the
  async ObjectServer operation succeeds. If `at()` or `remove()` returns an
  error, the in-process registry can disagree with the object server and affect
  later deltas. Relevant code: `niri-dbus/src/service.rs:88`,
  `niri-dbus/src/service.rs:113`, `niri-dbus/src/service.rs:124`,
  `niri-dbus/src/service.rs:135`.
- Medium: the service boundary is binary-only. D-Bus constants and property
  shapes are internal to `niri-dbus`, so shell consumers will need raw calls,
  duplicated constants, or generated bindings unless a small protocol/client
  crate is introduced.
- Medium: `paths::encode_segment` is valid-path-oriented but not injective; it
  can collide for names such as `-` and `_2D`, and for empty string versus `_`.
  Do not stabilize `output_path` until this is fixed.
- Low: `generation` increments on connect/reconnect but not disconnect, even
  though disconnect clears all dynamic state and emits `GenerationChanged`.
  Relevant code: `niri-dbus/src/state.rs:37`, `niri-dbus/src/state.rs:51`.
- Low: normal request/response reads do not special-case EOF, so a closed socket
  during `send_raw` becomes a JSON parse error over an empty string rather than
  a direct `UnexpectedEof`. Relevant code: `niri-dbus/src/ipc.rs:40`.
- Test gap: state and path tests are useful, but there are no D-Bus interface or
  ObjectManager tests covering interface names, property signatures, add/remove
  behavior, or property-change emission.

## Refactor Ideas

- Track state deltas at a finer granularity and emit only affected property
  changes, or explicitly batch coarse invalidation signals for shell consumers.
- Update object registration sets only after ObjectServer operations succeed,
  or make the registration operation idempotent and reconcile from the object
  server after failures.
- Split protocol constants/wire models/client helpers into a reusable library
  target before adding more shell consumers.
- Treat disconnect as a generation change if consumers use generation as a
  cache invalidation token.
- Keep IPC wrapper internals private and map request/response EOF explicitly.
- Treat niri reducer panics as stream desync requiring reconnect.

## Accepted Refactor Plan

1. Add `src/lib.rs` with stable contract helpers only:
   - D-Bus constants for bus, root path, and interfaces.
   - Public path helpers for root/output/workspace/window paths.
   - Typed key helper constants/functions for Locus-facing stable keys.
   - Rust client DTO/helpers should expose `Option<T>` rather than zero-or-one
     arrays when added.
2. Replace output path encoding with an injective segment encoding and add
   collision/golden tests.
3. Keep daemon internals private:
   - `ipc.rs`, `state.rs`, `dbus.rs`, and `service.rs` stay implementation
     modules.
   - `AsyncNiriSocket` is not exported.
4. Update lifecycle correctness:
   - `send_raw` returns `UnexpectedEof` on EOF.
   - `generation` increments on meaningful disconnects.
   - `apply_event` panics produce a reconnect/desync error rather than
     continuing the same stream.
   - Object registration sets change only after ObjectServer success.
5. Keep V1 read-only with coarse property emissions. Document that property
   changes are coarse invalidations until finer deltas are needed.
6. Add tests for path encoding collisions, generation-on-disconnect, and IPC EOF
   behavior where practical.

## Open Questions

- None for the accepted first refactor plan.

## Verification

- Implemented the accepted first refactor plan.
- `cargo test --manifest-path niri-dbus/Cargo.toml` passed: 8 tests plus
  doctests.
