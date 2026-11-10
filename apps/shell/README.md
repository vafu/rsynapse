# Rsynapse Shell

The main GTK4/Relm4 shell process: bar, OSD, workspace selector, selected
project UI, media overlay, and session/request handling.

```text
D-Bus services -> data-sources Observables -> Relm4 widgets
```

## Build

```sh
cargo check -p rsynapse-shell
cargo test -p rsynapse-shell
```

Install from the repository root with `./install/local.sh`.

## Structure

- `src/main.rs` — startup, launch mode, systray watcher host.
- `src/lib.rs` — app startup wrapper and module exports.
- `src/session.rs` — session-state re-exports.
- `src/theme.rs` — legacy theme helpers; no service access.
- `src/widgets/bar/` — panel surfaces and indicator widgets.
- `src/widgets/mpris/` — media overlay and controls.
- `src/widgets/workspace_selector/` — workspace picker.
- `src/widgets/osd/` — audio/brightness OSD.
- `stylesheets/` — shell SCSS entrypoint and surface styles.

Notifications are a separate app in `apps/notifications`. Launcher tooling is in
`apps/launcher`.

## Data Access

Widgets consume typed adapters from `apps/shared/data-sources`. Do not construct
`zbus` proxies, spawn service processes, or traverse locus relations in widget
code. Add missing adapters in `data-sources` so other apps can reuse them.

## Reactive UI

Follow `core/shell-core/CORE_WIDGETS_GUIDE.md` and
`src/widgets/AGENTS.md`:

- use `shell_macros::model`, `component`, `view_model`, and `source`;
- list structure is keyed and reconciled; item updates subscribe per row;
- avoid rebuilding a full list for a single item's property change.
