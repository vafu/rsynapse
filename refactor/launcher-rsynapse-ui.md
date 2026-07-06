# launcher-rsynapse-ui Review

Status: reviewed

## Scope

Launcher UI package.

## Findings

- **High - older search results can overwrite newer input.** Every search-entry
  change spawns a thread at `shell/launcher/rsynapse-ui/src/main.rs:109` and
  posts results back at `shell/launcher/rsynapse-ui/src/main.rs:115` without a
  query generation check or cancellation. A slow response for an earlier query
  can replace the displayed results for a later query.
- **Medium - activation performs blocking D-Bus work on the GTK update path.**
  `Msg::Activate` calls `dbus::execute` directly at
  `shell/launcher/rsynapse-ui/src/main.rs:241`; `dbus.rs` uses the blocking
  zbus API at `shell/launcher/rsynapse-ui/src/dbus.rs:1`.
- **Medium - the UI uses polling to bridge its D-Bus toggle thread into GTK.**
  The D-Bus object thread sends through an mpsc channel at
  `shell/launcher/rsynapse-ui/src/main.rs:162`, and GTK polls it every 100 ms at
  `shell/launcher/rsynapse-ui/src/main.rs:196`. This is inconsistent with the
  repo direction of typed async streams for live shell state.
- **Medium - the UI duplicates the launcher D-Bus contract.** Constants and raw
  tuple decoding live in `shell/launcher/rsynapse-ui/src/dbus.rs:3` and
  `shell/launcher/rsynapse-ui/src/dbus.rs:24` instead of a shared client crate.

## Refactor Ideas

- Convert launcher UI D-Bus access to async `zbus` integrated with Relm4 command
  streams, and tag/cancel search requests by query generation.
- Replace the polling toggle bridge with a GLib main-context channel or an async
  stream source.
- Share launcher protocol/client types with the CLI.

## Open Questions

- Should the launcher UI move onto `shell_core::ShellApp` and source helpers, or
  remain a separate nested launcher application?

## Verification

- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace` passed; 0
  UI tests, with a dead-code warning for `SearchResult::data`.
