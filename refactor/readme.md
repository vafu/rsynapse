# Rsynapse Review Session

This directory records the interactive review over every project in the
workspace. Keep observations durable here as each unit is reviewed.

## Global Constraints

- Current direction: `D-Bus services -> zbus async streams -> shell-core Observables -> Relm4 widgets`.
- Keep reusable shell framework code in `shell/core`.
- Keep Rsynapse product UI and policy in `shell/app` or adjacent shell UI crates.
- Keep niri protocol projection in `niri-dbus`.
- Keep cross-service associations in `locus`.
- Prefer `zbus`, typed structs/enums, async streams, `ObjectManager`, `Properties`, and explicit signals.
- Avoid filesystem-backed live IPC, polling, sleeps, debounce-based correctness, and manual edits to generated code.

## Review Queue

1. `locus` - reviewed
2. `niri-dbus` - reviewed
3. `shell/core/shell-core` - reviewed
4. `shell/core/background-effect` - reviewed
5. `shell/core/macros` - reviewed
6. `shell/core/rx-macros` - reviewed
7. `shell/app` - reviewed
8. `shell/examples/battery-status` - reviewed
9. `shell/examples/volume-status` - reviewed
10. `shell/examples/window-tiles` - reviewed
11. `shell/launcher/rsynapse-plugin` - reviewed
12. `shell/launcher/rsynapse-daemon` - reviewed
13. `shell/launcher/rsynapse-cli` - reviewed
14. `shell/launcher/rsynapse-ui` - reviewed
15. `shell/launcher/rsynapse-plugin-launcher` - reviewed
16. `shell/launcher/rsynapse-plugin-shell` - reviewed
17. `shell/launcher/rsynapse-plugin-calc` - reviewed
18. `shell/launcher/rsynapse-plugin-commands` - reviewed
19. `install` - reviewed

## Cross-Cutting Findings

- **Subscribe-before-read races repeat across D-Bus sources.** `shell-core`
  property/ObjectManager helpers and app-level Locus relation sources read
  initial state before subscribing to change signals. Fix the generic source
  ordering first, then remove local ad hoc relation sources where possible.
- **Protocol/client types are duplicated.** Niri, Locus, and launcher D-Bus
  constants and DTOs are redefined in services, app modules, examples, CLI, UI,
  and shell scripts. Shared protocol/client crates would reduce drift and make
  interface tests easier.
- **Dynamic launcher plugins are the largest legacy risk.** The plugin ABI is
  not FFI-safe, the daemon executes shell templates, and provider queries can
  run arbitrary synchronous code inside search. Decide whether this workspace
  remains a dynamic plugin launcher or becomes typed D-Bus/provider services.
- **Tests skew toward pure helpers and expansion strings.** Core source helpers
  have useful unit coverage, but D-Bus integration behavior, macro compile
  output, launcher daemon/client flows, install rendering, and GTK lifecycle
  paths are mostly untested.
- **Shell hot paths still contain polling/debounce/blocking work.** Examples,
  launcher UI, stylesheet reload, config reload, and helper scripts have small
  instances that conflict with the current typed async-stream direction.

## Completed Units

- [locus](locus.md)
- [niri-dbus](niri-dbus.md)
- [shell-core](shell-core.md)
- [gtk4-background-effect](gtk4-background-effect.md)
- [shell-macros](shell-macros.md)
- [shell-rx-macros](shell-rx-macros.md)
- [rsynapse-shell-app](rsynapse-shell-app.md)
- [shell-examples-battery-status](shell-examples-battery-status.md)
- [shell-examples-volume-status](shell-examples-volume-status.md)
- [shell-examples-window-tiles](shell-examples-window-tiles.md)
- [launcher-rsynapse-plugin](launcher-rsynapse-plugin.md)
- [launcher-rsynapse-daemon](launcher-rsynapse-daemon.md)
- [launcher-rsynapse-cli](launcher-rsynapse-cli.md)
- [launcher-rsynapse-ui](launcher-rsynapse-ui.md)
- [launcher-rsynapse-plugin-launcher](launcher-rsynapse-plugin-launcher.md)
- [launcher-rsynapse-plugin-shell](launcher-rsynapse-plugin-shell.md)
- [launcher-rsynapse-plugin-calc](launcher-rsynapse-plugin-calc.md)
- [launcher-rsynapse-plugin-commands](launcher-rsynapse-plugin-commands.md)
- [install](install.md)

## Verification Summary

- `cargo test --manifest-path locus/Cargo.toml`
- `cargo test --manifest-path niri-dbus/Cargo.toml`
- `cargo test -p shell-core --manifest-path shell/Cargo.toml`
- `cargo test -p gtk4-background-effect --manifest-path shell/Cargo.toml`
- `cargo test -p shell-macros --manifest-path shell/Cargo.toml`
- `cargo test -p shell-rx-macros --manifest-path shell/Cargo.toml`
- `cargo test -p rsynapse-shell --manifest-path shell/Cargo.toml`
- `cargo test -p rsynapse-battery-status-example --manifest-path shell/Cargo.toml`
- `cargo test -p rsynapse-volume-status-example --manifest-path shell/Cargo.toml`
- `cargo test -p rsynapse-window-tiles-example --manifest-path shell/Cargo.toml`
- `cargo test --manifest-path shell/launcher/Cargo.toml --workspace`
- `bash -n install/local.sh`
- `bash -n install/bin/proj`
