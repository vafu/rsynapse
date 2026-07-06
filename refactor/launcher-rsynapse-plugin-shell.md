# launcher-rsynapse-plugin-shell Review

Status: reviewed

## Scope

Shell launcher plugin implementation.

## Findings

- **High - shell command quoting is incorrect before it reaches the daemon.**
  The plugin validates the user query as shell syntax at
  `shell/launcher/rsynapse-plugin-shell/src/lib.rs:12`, then wraps the original
  command with `format!("sh -c '{}'", command)` at
  `shell/launcher/rsynapse-plugin-shell/src/lib.rs:44`. Queries containing a
  single quote or shell metacharacters can change the command that the daemon
  later runs through another `sh -c`.
- **Medium - query-time syntax validation spawns `sh` for every candidate
  query.** `is_valid_shell_syntax` runs a process at
  `shell/launcher/rsynapse-plugin-shell/src/lib.rs:12`. In the GTK launcher this
  can happen on every search-entry change.

## Refactor Ideas

- Treat shell execution as an explicit action type with argv data, or keep one
  clearly marked shell-provider path where the original query is passed directly
  to a single `sh -c`.
- Add unit tests for quoting cases, especially commands containing single
  quotes.

## Open Questions

- Should a shell-executor provider be enabled by default in the current
  Rsynapse shell, or should it be opt-in config?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  plugin-shell tests, with the shared FFI-safety warning.
