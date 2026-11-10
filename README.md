# Rsynapse

Rsynapse is a Rust-first desktop session stack: D-Bus services, reactive typed
data adapters, and GTK4/Relm4 apps.

```text
D-Bus services -> data-sources Observables -> Relm4 widgets
```

The repository is one root Cargo workspace.

## Layout

- `apps/shell/` — main bar, OSD, workspace selector, and media overlay.
- `apps/notifications/` — notification daemon and notification center.
- `apps/launcher/` — launcher daemon, CLI, UI, and plugins.
- `apps/shared/` — app-kit, data-sources, desktop-entry, and icon picker.
- `apps/examples/` — small reactive GTK examples.
- `core/reactive/` — Observable runtime, state signals, and list reconciliation.
- `core/shell-core/` — generic Relm4/GTK integration.
- `core/macros/`, `core/rx-macros/` — reactive UI and `dbus_model` macros.
- `dbus/locus/` — session relation store and CLI.
- `dbus/projd/` — project-management service, model, and `proj` CLI.
- `dbus/workspace-dbus/` — compositor-neutral workspace projection.
- `dbus/workspace-policy/` — shared workspace naming/visibility policy.
- `misc/` — unsettled projects: steward, steward-api, dashboard-api,
  observability.
- `install/` — user-local installer, systemd units, and D-Bus activation.

## Data Boundary

Apps observe services through `apps/shared/data-sources`. Shell widgets and
launcher plugins must not use `zbus` directly or spawn service-data processes.
App-owned D-Bus server implementations remain in their app.

`core/reactive` is service-agnostic; do not add service adapters there.

## Build And Test

```sh
cargo check --workspace
cargo test --workspace
scripts/check-data-access.sh
scripts/check.sh all
```

For an external build cache:

```sh
CARGO_TARGET_DIR=/tmp/rsynapse-target cargo test --workspace
```

`nix develop` provides native GTK/libadwaita/gtk4-layer-shell dependencies.

## Runtime Services

- `org.rsynapse.Engine` from the launcher daemon.
- `org.rsynapse.Workspace` from `rsynapse-workspace-dbus`.
- `org.rsynapse.Locus` from `rsynapse-locusd`.
- `org.rsynapse.Proj` from `rsynapse-projd`.
- `org.rsynapse.Steward` from `rsynapse-steward`.
- `rsynapse-shell` for the bar/OSD process.
- `rsynapse-notifications` for notification popups and center.

## Local Install

```sh
./install/local.sh
```

See `install/README.md` for paths and overrides.
