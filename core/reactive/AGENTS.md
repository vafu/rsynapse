# shell-reactive

UI-agnostic reactive machinery.

- Owns `Observable`, `Source`, `StateSignal`, `shared_by_key`, list diffing, and
  source lifecycle/error reporting.
- Do not add service clients, D-Bus descriptors, process execution, or product
  policy here.
- Generated `dbus_model` code uses runtime paths from this crate and service
  descriptors from `data-sources`.
- Keep cancellation and replay semantics explicit; do not rely on polling.
