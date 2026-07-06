# launcher-rsynapse-cli Review

Status: reviewed

## Scope

Launcher command-line client package.

## Findings

- **Medium - the CLI duplicates the launcher D-Bus wire contract.**
  `shell/launcher/rsynapse-cli/src/main.rs:6` declares its own `Engine` proxy
  and raw tuple result type. The UI duplicates the same contract separately, so
  daemon/API changes can silently desynchronize clients.
- **Medium - `exec` inherits the daemon's global latest-search cache semantics.**
  The README documents that `exec` only works if the result is still present in
  the daemon's latest cached search results at
  `shell/launcher/rsynapse-cli/README.md:20`. That is surprising for a CLI,
  where search and execute are often separate commands.

## Refactor Ideas

- Move the D-Bus proxy and `SearchResult` DTO into a shared launcher client
  crate.
- Add a CLI command that performs search-and-execute in one daemon call, or use
  stable provider payloads so `exec` does not depend on global cache state.

## Open Questions

- Should the CLI be retained as a launcher debugging tool only, or should it be
  a stable user interface with predictable noninteractive behavior?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  CLI tests.
