# rsynapse-shell-app Review

Status: reviewed

## Scope

Combined shell application, notifications binary, request socket, styles, and
Rsynapse-specific UI policy.

## Review Order

1. `shell/app/Cargo.toml`
2. `shell/app/src/main.rs`
3. `shell/app/src/bin/rsynapse-notifications.rs`
4. `shell/app/src/lib.rs`
5. `shell/app/src/request.rs`
6. `shell/app/src/theme.rs`
7. `shell/app/src/hints.rs`
8. `shell/app/src/desktop_icon.rs`
9. `shell/app/src/widgets/bar/*`
10. `shell/app/src/widgets/notifications/*`
11. `shell/app/src/widgets/osd/*`
12. `shell/app/src/widgets/material_icon.rs`

## Crate Map

- `src/main.rs` runs the bar process and dispatches `request` CLI invocations.
- `src/bin/rsynapse-notifications.rs` runs the notification process and shares
  the same request CLI path.
- `src/lib.rs` owns app setup, tracing, optional chrome/pprof profiling,
  stylesheet/theme setup, and app builder defaults.
- `src/request.rs` owns the local Unix request socket protocol for shell and
  notification processes.
- `src/theme.rs`, `src/hints.rs`, and `src/desktop_icon.rs` own product policy
  for theme toggling, hint state, and app-id icon lookup.
- `src/widgets/bar/*` owns the main bar UI and local widget source providers.
- `src/widgets/notifications/*` owns the notification D-Bus service, window,
  cards, and notification model.
- `src/widgets/osd/*` owns volume/brightness OSD display.
- `src/widgets/material_icon.rs` lazily resolves Material Symbol icons.

## Findings

- High: notification action buttons emit `ActionInvoked` from a fresh session
  bus connection instead of from the owned `org.freedesktop.Notifications`
  service/object. Clients that match the notification server as sender can miss
  the signal. Relevant code: `shell/app/src/widgets/notifications/card.rs:195`.
- Medium: the bar notification indicator is currently wired to a constant
  `false`, so the clock dot never reflects notification state. Relevant code:
  `shell/app/src/widgets/notifications/mod.rs:28`.
- Medium: Locus relation sources perform an initial D-Bus read before installing
  relation signal streams, so relation changes in that window can be missed.
  This affects workspace project labels and window-agent tiles. Relevant code:
  `shell/app/src/widgets/bar/project_label/source/project.rs:63`,
  `shell/app/src/widgets/bar/project_label/source/project.rs:65`,
  `shell/app/src/widgets/bar/window_tile/agent/source/actual.rs:92`,
  `shell/app/src/widgets/bar/window_tile/agent/source/actual.rs:94`.
- Medium: Locus wire structs and relation constants are duplicated in app
  source modules instead of coming from a shared Locus protocol/client crate.
  This will drift as `locus` evolves.
- Medium: `material_icon::icon_name` can trigger runtime network access,
  filesystem writes, and `gtk-update-icon-cache` from the shell process when an
  icon is missing. Useful during development, but risky for normal shell hot
  paths and offline startup. Relevant code:
  `shell/app/src/widgets/material_icon.rs:114`,
  `shell/app/src/widgets/material_icon.rs:136`,
  `shell/app/src/widgets/material_icon.rs:186`.
- Medium: the request socket falls back to a predictable path under `/tmp` when
  `XDG_RUNTIME_DIR` is missing and does not check peer credentials or enforce a
  request size cap. The normal session case should have `XDG_RUNTIME_DIR`, but
  the fallback is wider than a shell control socket should be. Relevant code:
  `shell/app/src/request.rs:131`, `shell/app/src/request.rs:264`.
- Low: `shell/app/src/widgets/bar/mod.rs` is over 1,100 lines and already has a
  local TODO to split the right cluster. This is now large enough that macro
  expansion errors and review of UI behavior are harder than they need to be.

## Refactor Ideas

- Route notification actions back through the notification service state so
  `ActionInvoked` is emitted by the service-owned object/sender.
- Replace `has_notification_items()` with an actual cross-process source, likely
  through the notification request/D-Bus service or a small shell-visible
  notification state projection.
- Move Locus client logic into a shared typed client/protocol crate and use the
  same subscribe-before-read pattern chosen for `shell-core` D-Bus sources.
- Preinstall Material icons in `install/` or during build/dev tooling, and keep
  runtime lookup offline-only.
- Require `XDG_RUNTIME_DIR` for request sockets, or create a private `0700`
  runtime directory and add length/peer checks.
- Split the bar right cluster into child components once the macro/component
  ownership issue noted in the TODO is resolved.

## Open Questions

- Should notifications expose state to the bar via D-Bus, the existing request
  socket, or a small `shell-core` source shared by both processes?
- Is runtime Material icon fetching intended only as a development convenience?

## Verification

- `cargo test -p rsynapse-shell --manifest-path shell/Cargo.toml` passed: 37
  tests, plus both binaries with 0 tests.
