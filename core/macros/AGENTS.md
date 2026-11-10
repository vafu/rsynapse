# shell-macros

Procedural macros for reactive Relm4 components and D-Bus models.

- Generated runtime paths use `::shell_reactive`; generated service descriptor
  paths use `::data_sources::dbus`.
- `dbus_model` is for typed property adapters; producers still live in
  `apps/shared/data-sources`.
- Update macro expansion tests whenever generated paths or signatures change.
