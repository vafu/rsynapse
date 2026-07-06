# launcher-rsynapse-plugin-launcher Review

Status: reviewed

## Scope

Launcher plugin implementation.

## Findings

- **High - desktop `Exec` strings are passed through as executable data.**
  `parse_desktop_file` stores the raw desktop entry `Exec` value at
  `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:122`, and the plugin
  exposes it as result data at
  `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:186`. With the daemon's
  default `{data}` execution template, this bypasses freedesktop field-code
  parsing and structured argv handling.
- **Medium - indexing only scans one directory level.** `reindex` uses
  `fs::read_dir` at `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:61`.
  The unused `find_and_parse_apps` path uses `WalkDir` at
  `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:93`, so the current
  implementation can miss nested application entries and has stale code for the
  intended traversal.
- **Low - watcher setup can panic and is nonrecursive.**
  `start_watcher_thread` unwraps watcher creation and `watch` registration at
  `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:138` and
  `shell/launcher/rsynapse-plugin-launcher/src/lib.rs:143`.

## Refactor Ideas

- Parse desktop `Exec` into a structured launch request rather than a shell
  command string.
- Delete the unused `find_and_parse_apps` path or make `reindex` use the same
  recursive traversal.
- Add tests around desktop parsing, hidden entries, nested desktop files, and
  field-code handling.

## Open Questions

- Should application launching move to `gio::AppInfo` or another desktop-aware
  launch API instead of daemon-managed shell execution?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  plugin-launcher tests, with unused import/dead-code warnings.
