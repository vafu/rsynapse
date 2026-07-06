# launcher-rsynapse-daemon Review

Status: reviewed

## Scope

Launcher daemon package.

## Findings

- **High - `Execute` is tied to a single mutable global search cache.**
  `Search` clears and replaces `last_results` at
  `shell/launcher/rsynapse-daemon/src/main.rs:132`, while `Execute` resolves an
  id only against that latest cache at
  `shell/launcher/rsynapse-daemon/src/main.rs:150`. Multiple clients, fast UI
  typing, or a CLI `exec` after another client searches can execute the wrong
  cached item or fail even though the user selected a valid result.
- **High - execution templates are interpolated into `sh -c` without structured
  argument handling.** The daemon replaces placeholders directly at
  `shell/launcher/rsynapse-daemon/src/main.rs:177` and spawns
  `sh -c` at `shell/launcher/rsynapse-daemon/src/main.rs:186`. Desktop `Exec`
  strings, command-plugin output, titles, and user config all flow through this
  path, so quoting and injection behavior are part of the runtime contract.
- **High - plugin loading trusts every `.so` in the selected plugin directory.**
  `load_plugins_from` iterates all `.so` files at
  `shell/launcher/rsynapse-daemon/src/main.rs:69` and loads them with
  `Library::new` at `shell/launcher/rsynapse-daemon/src/main.rs:77`. In release
  mode this is `~/.local/lib/rsynapse/plugins` from
  `shell/launcher/rsynapse-daemon/src/main.rs:217`; any file there runs in the
  daemon process.
- **Medium - D-Bus work is synchronous inside async methods.** `search` calls
  plugin `query` implementations inline at
  `shell/launcher/rsynapse-daemon/src/main.rs:122`; plugins may scan files,
  spawn commands, or take locks. One slow provider blocks the daemon method
  handler.
- **Low - config reload relies on a debounce for correctness.** The watcher
  drops events if `last_reload.elapsed().as_millis() <= 100` at
  `shell/launcher/rsynapse-daemon/src/main.rs:298`, which conflicts with the
  workspace preference to avoid debounce-based correctness.

## Refactor Ideas

- Replace `Execute(id)` with an explicit result token/session, or make execute
  route back to the provider with stable typed payloads rather than a shared
  daemon cache.
- Replace shell template execution with structured command specs and argument
  arrays. Keep `sh -c` only for an explicit shell-provider mode.
- Move provider execution to bounded async tasks or provider-owned services so
  D-Bus request handlers do not run arbitrary plugin code inline.

## Open Questions

- Should this launcher daemon remain under `shell/launcher`, or should launcher
  search become one D-Bus service in the same style as planned `niri-dbus` and
  `locus`?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  daemon tests.
