# Rsynapse

Rsynapse is a Rust-first desktop session workspace. It combines small D-Bus
services with GTK4/Relm4 shell surfaces and launcher tooling.

The active direction is:

```text
D-Bus services -> zbus async streams -> shell-core Observables -> Relm4 widgets
```

The repository is intentionally split into several focused projects rather
than one top-level Cargo workspace.

## Projects

- `shell/`
  The shell UI monorepo. It contains reusable GTK/Relm4 framework crates under
  `shell/core`, the current combined shell app under `shell/app`, and the
  launcher workspace under `shell/launcher`.

- `niri-dbus/`
  A session D-Bus projection service for niri IPC state. It exposes outputs,
  workspaces, windows, focus, and related events as D-Bus objects/properties.

- `locus/`
  A small session D-Bus relation store for typed associations between external
  objects, such as workspace -> project or window -> agent.

- `install/`
  User-local install scripts, D-Bus activation templates, systemd user units,
  and helper scripts.

## Boundaries

- Keep reusable shell framework code in `shell/core`.
- Keep concrete Rsynapse shell UI in `shell/app`, `shell/launcher`, or future
  surface crates under `shell/`.
- Keep niri protocol projection in `niri-dbus`.
- Keep cross-service relation storage in `locus`.
- Keep service boundaries typed and D-Bus-first; do not rebuild filesystem IPC
  for live shell state.

## Build And Test

Each project can be checked independently:

```sh
cargo test --manifest-path locus/Cargo.toml
cargo test --manifest-path niri-dbus/Cargo.toml
cargo test --manifest-path shell/Cargo.toml --workspace
cargo test --manifest-path shell/launcher/Cargo.toml --workspace
```

For repeated shell builds, use an external target directory to avoid polluting
the workspace:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target \
  cargo test --manifest-path shell/Cargo.toml --workspace
```

## Local Install

The local installer installs release binaries, launcher plugins, D-Bus
activation files, systemd user units, and helper scripts into user-local paths:

```sh
./install/local.sh
```

See `install/README.md` for the exact paths and override knobs.

## Runtime Services

The current session services and surfaces are:

- `org.rsynapse.Engine` from the launcher daemon.
- `org.rsynapse.Niri` from `niri-dbus`.
- `org.rsynapse.Locus` from `locus`.
- `rsynapse-shell` for the main shell bar/OSD process.
- `rsynapse-notifications` for notification popups and notification center.

All D-Bus APIs are still internal and unstable.
