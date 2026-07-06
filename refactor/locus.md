# locus Review

Status: reviewed

## Scope

Session D-Bus relation service. It should store typed associations between
objects exposed by other services and emit relation change signals. It should
not become a general graph mirror or a second D-Bus.

## Review Order

1. `locus/README.md`
2. `locus/Cargo.toml`
3. `locus/src/main.rs`
4. `locus/src/service.rs`
5. `locus/src/store.rs`

## Crate Map

- `src/main.rs` initializes tracing and delegates to the service runner.
- `src/service.rs` owns the `org.rsynapse.Locus` session bus name, exports
  `/org/rsynapse/Locus`, defines the `org.rsynapse.Locus.Relations1` D-Bus
  interface, maps store I/O errors to FDO errors, and emits relation/property
  signals.
- `src/store.rs` owns the in-memory relation vector, JSON persistence,
  validation, timestamping, query methods, and unit tests.

## User Notes

- Commit to a typed endpoint shape now rather than shipping opaque strings as
  the only API.
- Treat "signals only after persistence succeeds" as a hard service contract.
- Add a reusable library/client API right away, before shell clients start
  duplicating Locus constants and wire types.
- `src/lib.rs` should expose only stable DTO/client types. Daemon internals such
  as service runtime and storage implementation should stay private.
- Keep the crate and binary name as `locus`; no split naming is needed right now.
- Use an explicit typed endpoint enum shape, not a flatter struct with optional
  fields.
- Make `StorePath` private/debug-only rather than part of the stable public
  D-Bus/client API.
- For `Clear`, emit removed-record information so cache consumers can update
  precisely. Whether to keep a coarse `RelationCleared` signal as well is still
  open.
- The typed endpoint change may break the current persisted JSON shape. No
  automatic migration is required; existing local state can be fixed manually.
- Endpoint variants accepted for the first API:
  `StableKey { kind: String, id: String }` and
  `DBusObject { bus: String, service: String, path: String, interface: String }`.
- Do not add stronger file/directory fsync durability for now. The important
  guarantee is process-visible transactionality: persist the next state, then
  swap it into memory only after persistence succeeds.
- `Clear` should emit per-record removals plus a final coarse completion signal.
- `SetOne` should guarantee signal order: removed records first, then the
  added/updated replacement target.
- The relation-name validation question is considered resolved for this review
  pass; no separate open question remains.

## Findings

- High: failed persistence can leave the running service state out of sync with
  disk. `set`, `set_one`, `unset`, and `clear` mutate `self.records` before
  `persist()` succeeds, so an I/O failure returns an error to the D-Bus caller
  but subsequent queries against the running process can still observe the
  failed change. Relevant code: `locus/src/store.rs:49`, `locus/src/store.rs:87`,
  `locus/src/store.rs:112`, `locus/src/store.rs:129`.
- Medium: the service boundary is not yet reusable as a typed Rust API.
  `RelationRecord` and the D-Bus interface live inside a binary crate, so shell
  clients will either duplicate wire types or use raw D-Bus calls. That weakens
  the repo direction of typed service boundaries.
- Medium: object references are currently plain strings. The repo direction
  allows stable typed keys, but also calls out bus/service/object-path/interface
  references for external D-Bus objects. `RelationRecord` has no typed way to
  distinguish these forms yet.
- Low: `clear` emits `RelationCleared(subject, relation, removed_count)` rather
  than the removed records. Stream consumers that cache relation state cannot
  update precisely from the signal alone and must query after receiving it.
- Low: `StorePath` is exposed as a D-Bus property today, but the accepted API
  direction treats it as debug/private implementation detail rather than stable
  service contract.
- Test gap: coverage is currently store-unit focused. There are no D-Bus
  interface tests for methods, property changes, or signal payloads/order.

## Refactor Ideas

- Make mutation transactional at the store layer: compute the next record set
  first, persist it, then swap it into `self.records` only after success.
- Split the public wire model and client helpers into a small library target or
  crate before shell clients depend on `locus`.
- Introduce a typed relation endpoint model now, likely an enum covering stable
  keys and D-Bus object references, while keeping the D-Bus representation
  compatible with zvariant.
- Emit per-record removal information from `clear`. Decide whether the coarse
  `RelationCleared` signal stays as a batch/completion signal or is removed from
  the stable API.

## Accepted Refactor Plan

1. Add `src/lib.rs` exposing stable API only:
   - D-Bus constants for service, path, and interface.
   - `RelationEndpoint` enum with `StableKey` and `DBusObject` variants.
   - `RelationRecord` DTO using typed `subject` and `target` endpoints.
   - A typed async client/proxy wrapper for `Set`, `SetOne`, `Unset`, `Clear`,
     `Targets`, `Subjects`, and `List`.
2. Keep daemon internals private:
   - `service.rs` and `store.rs` remain implementation modules for the binary.
   - `StorePath` is removed from the stable API or kept only as a private/debug
     implementation detail.
3. Update store transactionality:
   - Build the next `Vec<RelationRecord>` without mutating `self.records`.
   - Persist the next state.
   - Swap `self.records` only after persistence succeeds.
   - Add tests that force persistence failure and assert in-memory state does
     not change.
4. Update D-Bus signal behavior:
   - Signals are emitted only after a store method has persisted and committed.
   - `Clear` emits each removed record and then a final coarse clear/completion
     signal.
   - `SetOne` guarantees removed-record signals before the added/updated target
     signal.
5. Update persistence format directly for typed endpoints:
   - No automatic migration from current string `subject`/`target` JSON.
   - Existing local `relations.json` can be manually rewritten or deleted.
6. Add tests:
   - Endpoint serialization/deserialization and validation.
   - Transactional persistence failure behavior.
   - Store-level `clear` removal list ordering.
   - D-Bus method/signal tests for ordering once the client API exists.

## Open Questions

- None for the accepted first refactor plan.

## Verification

- Implemented the accepted first refactor plan.
- `cargo test --manifest-path locus/Cargo.toml` passed: 10 tests plus
  doctests.
