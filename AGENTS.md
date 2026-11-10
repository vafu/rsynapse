# Rsynapse Workspace

One root Cargo workspace for the Rust-first Rsynapse desktop session.

```text
apps/
  shell/                    main bar, OSD, workspace selector, media overlay
  notifications/            standalone notification daemon and center
  launcher/                 launcher daemon, CLI, UI, and plugins
  shared/
    app-kit/                shared GTK app startup, tracing, request socket
    data-sources/           all D-Bus/process/service adapters and writes
    desktop-entry/          desktop entry parsing and launch helpers
    nerd-icon-picker/       icon catalog and picker library
  examples/                 small GTK source examples
core/
  reactive/                 Observable runtime, StateSignal, list reconciliation
  shell-core/               Relm4 integration: app/model/list/window/CSS helpers
  macros/                   model/view/source/dbus_model macros
  rx-macros/                combine_latest and reactive authoring macros
dbus/
  locus/                    session relation service and CLI
  projd/                    project-management service, model, and CLI
  workspace-dbus/           compositor-neutral workspace projection
  workspace-policy/         shared workspace naming/project policy
misc/
  steward/                  session metrics/listener side effects
  steward-api/              steward D-Bus contract
  dashboard-api/            HTTP/SSE + Grafana adapter
  observability/            Graphite/Grafana deployment
install/                    user-local installer, units, activation files
scripts/                    repository checks and helpers
docs/                       repository-level design notes
```

## Boundaries

- UI code consumes typed adapters from `apps/shared/data-sources`. Widgets,
  plugins, and notification UI must not use `zbus`, spawn service processes, or
  read service state directly.
- `core/reactive` is service-agnostic machinery. Do not add D-Bus, process, or
  filesystem adapters to it.
- `core/shell-core` is generic Relm4 integration. Do not add Rsynapse-specific
  surfaces or service clients here.
- Apps own product UI and app-local D-Bus server implementations. Launcher
  command/shell plugins may execute user commands; those are actions, not data
  observation.
- `dbus/` owns service implementations and contracts. Keep project state in
  projd, relations in locus, and compositor projection in workspace-dbus.
- `misc/` is unsettled. Do not infer a strong architecture from it.

## Workflow

- Read the nearest `AGENTS.md` before editing a subtree.
- Use `scripts/check-data-access.sh` when touching app data access.
- Verify with `cargo check --workspace`, focused crate tests, then
  `cargo test --workspace`.
- Keep every `AGENTS.md` short, current, and useful. It is working context, not
  documentation exhaust; remove stale detail instead of letting it rot.
