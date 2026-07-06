# shell-macros Review

Status: reviewed

## Scope

Proc macro support crate for shell framework and application code.

## Review Order

1. `shell/core/macros/Cargo.toml`
2. `shell/core/macros/src/lib.rs`
3. `shell/core/macros/src/dbus_model.rs`
4. `shell/core/macros/src/view_model.rs`
5. `shell/core/macros/src/locus_bindings/config.rs`
6. `shell/core/macros/src/locus_bindings/component.rs`
7. `shell/core/macros/src/locus_bindings/view.rs`
8. `shell/core/macros/src/locus_bindings/expand.rs`
9. `shell/core/macros/src/locus_bindings/test.rs`

## Crate Map

- `src/lib.rs` exposes the proc macro entry points: `bindings`, `component`,
  `model`, `view_model`, and `dbus_model`.
- `src/dbus_model.rs` generates D-Bus model wrappers and zbus proxy traits from
  traits or named-field structs.
- `src/view_model.rs` wires `Source<T>` view-model streams into Relm4 component
  lifecycle.
- `src/locus_bindings/config.rs` parses source-binding, component, and typed
  model macro configuration.
- `src/locus_bindings/component.rs` injects subscription startup/update methods
  into Relm4 component impls.
- `src/locus_bindings/view.rs` transforms `view!` token streams for source
  setters and list bindings.
- `src/locus_bindings/expand.rs` generates source state modules, typed source
  models, dirty masks, subscription startup, and update dispatch.

## Findings

- High: async direct component bindings can generate uncompilable code.
  `inject_start_call` selects `start_async` for async components, but
  `expand_locus_module` only generates `pub fn start(...)`; no matching
  `start_async` is emitted for non-model direct bindings. Relevant code:
  `shell/core/macros/src/locus_bindings/component.rs:146`,
  `shell/core/macros/src/locus_bindings/expand.rs:222`.
- High: typed models with nested model fields lose nested subscriptions in async
  components. The sync `start` method includes `#(#nested_watchers)*`, but
  `start_async` only includes `#(#async_watchers)*`. Relevant code:
  `shell/core/macros/src/locus_bindings/expand.rs:651`,
  `shell/core/macros/src/locus_bindings/expand.rs:668`.
- Medium: macro tests are useful but mostly assert expansion strings. They do
  not compile expanded code through representative sync/async Relm4 components,
  which is why the missing `start_async` path can survive the test suite.
- Low: dirty-mask validation checks direct source bindings and nested bindings
  separately, but the generated `Field` enum contains both. A typed model with
  more than 128 combined source+nested fields can still generate a `u128` shift
  beyond the supported mask width. Relevant code:
  `shell/core/macros/src/locus_bindings/config.rs:374`,
  `shell/core/macros/src/locus_bindings/expand.rs:525`.
- Low: `dbus_model` maps `Option<T>` to zero-or-one array wire values. That
  matches the current Niri-style optional property encoding, but it is
  surprising for general D-Bus models unless documented at macro call sites.

## Refactor Ideas

- Generate direct-binding `start_async` alongside `start`, or have
  `inject_start_call` use `start` only when the generated sender type matches.
- Add nested async watcher generation that maps nested messages into
  `AsyncComponent` input, mirroring the sync path.
- Add `trybuild`-style compile tests for representative macro consumers:
  sync direct bindings, async direct bindings, sync typed model, async typed
  model, nested typed model, and expected-failure diagnostics.
- Validate the combined generated `Field` count for typed models.
- Document the macro's optional D-Bus property convention, or add a separate
  attribute for Niri zero-or-one-array properties.

## Open Questions

- Should the older `bindings`/`component` Observable macros keep growing, or
  should new generated code prefer `view_model`/`Source<T>` and leave the
  Observable path in maintenance mode?

## Verification

- `cargo test -p shell-macros --manifest-path shell/Cargo.toml` passed: 32
  tests.
