# Shell Project Notes

This file records high-level ownership and design intent for the shell app.

## Ownership

`apps/shell` owns the main desktop bar and its overlay surfaces:

- bar layout and indicators;
- OSD/audio/brightness overlays;
- workspace selector;
- selected-project display and project initialization actions;
- media overlay controls;
- shell request socket dispatch.

It does not own service implementations, notification policy, or launcher
plugins.

## Framework Split

- `core/reactive`: Observable runtime, `StateSignal`, list reconciliation.
- `apps/shared/data-sources`: all D-Bus/process/service adapters.
- `core/shell-core`: generic Relm4 app/list/window/CSS integration.
- `core/macros`: model/component/source/dbus_model expansions.
- `apps/shared/app-kit`: startup, tracing/profiling, request socket.

Widget-local `source.rs` modules should only compose data sources into view
models and display policy.

## UI Direction

Prefer canonical Adwaita/GTK widgets and native list semantics over custom
hover/selection chrome. Lists should reconcile by stable identity and each row
should subscribe to its own live state.

## Historical Note

The old combined shell monorepo, Remarked overlay, rnotes CLI, and task-notes
components were removed. Those experiments are not part of this app.
