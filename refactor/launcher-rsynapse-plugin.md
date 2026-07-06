# launcher-rsynapse-plugin Review

Status: reviewed

## Scope

Shared launcher plugin API crate.

## Findings

- **High - the plugin API is not FFI-safe even though all bundled plugins are
  loaded as `cdylib`s.** The crate exposes a Rust trait object contract at
  `shell/launcher/rsynapse-plugin/src/lib.rs:17`, and each plugin returns
  `*mut dyn Plugin` from an `extern "C"` symbol. `cargo test` warns that trait
  objects have no C equivalent. This can break across compiler versions,
  dependency graph drift, or rebuild mismatches even when the symbol loads.
- **Medium - D-Bus and plugin result types are split.** `ResultItem` is a plain
  Rust struct at `shell/launcher/rsynapse-plugin/src/lib.rs:8`, while the daemon
  has its own `DbusResultItem`. The comment at
  `shell/launcher/rsynapse-plugin/src/lib.rs:3` still describes this as a
  simplification rather than the intended protocol shape.

## Refactor Ideas

- Decide whether launcher plugins remain dynamic libraries. If yes, put a real
  ABI boundary in front of them (`abi_stable`, C-compatible vtables, or a
  process/D-Bus plugin protocol). If no, convert bundled plugins into normal
  crates linked into the daemon.
- Move result DTOs and D-Bus constants into a launcher protocol/client crate so
  daemon, CLI, and UI share the same wire contract.

## Open Questions

- Is dynamic plugin loading still important for the current Rsynapse direction,
  or should launch/search providers become ordinary D-Bus services?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed, but
  emitted FFI-safety warnings for plugin entry points.
