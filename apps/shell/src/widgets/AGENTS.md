# Shell Widgets

Read `../AGENTS.md` and `../../../core/shell-core/CORE_WIDGETS_GUIDE.md` first.

## Data Access

No `zbus`, service processes, filesystem polling, or Locus traversal in widget
code. Add or extend a typed adapter in `apps/shared/data-sources`; compose its
Observable here.

Widget-local `source.rs` modules only translate shared sources into view models
and display policy.

## Lists

Structure reconciles; rows subscribe. A list source provides stable keys only.
Each row owns live item data through `#[source(keyed_source(key))]`. Never
rebuild the whole list for a property update, and never insert/remove child
widgets from a full snapshot in `update` or `init`.

Use `shell_core::list::{ComponentListBoxExt, ComponentListUpdate}` and keyed
reconciliation. Prefer native Adwaita/GTK list widgets and styling.

## Module Shape

- Keep one meaningful component or submodel per module.
- `mod.rs` exposes the component and its init/view types.
- Keep display formatting local unless another widget needs it.
- Keep files under roughly 300 lines where practical.
- Do not hardcode child heights; use natural sizing, padding, and alignment.
