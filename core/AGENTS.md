# Core

Framework crates used by apps. No service-specific adapters or Rsynapse product
surfaces belong here.

- `reactive/`: Observable runtime, `StateSignal`, source lifecycle/error
  handling, keyed lists, and list diffing.
- `shell-core/`: generic Relm4 integration for apps, models, keyed lists,
  windows, and CSS.
- `macros/`: `model`, `component`, `view_model`, `source`, and `dbus_model`
  authoring macros.
- `rx-macros/`: reactive authoring helpers such as `combine_latest`.

Keep `reactive` free of D-Bus/process/filesystem adapters. Put those in
`apps/shared/data-sources`.
