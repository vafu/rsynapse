# Rsynapse Desktop Environment Workspace

This directory contains desktop-environment projects that work together as a
small, Rust-first session stack. Prefer D-Bus and typed async streams for live
reactive state. Do not rebuild a filesystem-backed IPC layer for shell hot
paths.

## Direction

The current direction is:

```text
D-Bus services -> zbus async streams -> shell-core Observables -> Relm4 widgets
```

`shell/` is the shell UI monorepo. It contains reusable GTK/Relm4 framework
crates under `shell/core`, the current combined shell app under `shell/app`,
and the launcher workspace under `shell/launcher`. Concrete Rsynapse surfaces
live outside the framework crates but inside `shell/` when they are UI
components.

## Projects

- `shell/`
  The shell UI monorepo. `shell/core` owns reusable framework crates,
  `shell/app` owns the current combined bar, OSD, notifications, request
  socket, styles, and Rsynapse-specific UI policy, and `shell/launcher` owns the
  launcher workspace. Package names such as `rsynapse-daemon` may be renamed
  later.

- `niri-dbus/`
  A planned session D-Bus projection for niri IPC. It should expose outputs,
  workspaces, windows, focus, and events as D-Bus objects/properties/signals.
  It must not contain shell UI policy.

- `locus/`
  A planned session D-Bus relation service. It stores typed associations
  between objects exposed by other services, such as workspace -> project or
  window -> agent. It should store relations and emit relation change signals;
  source services remain authoritative for their own properties.

## Relation Service Shape

Use D-Bus object semantics directly:

- External objects are identified by bus, service, object path, and interface,
  or by an explicitly typed stable key when object paths are ephemeral.
- Relations are typed names, preferably reverse-DNS names.
- The service should support setting, unsetting, querying targets, querying
  subjects, and subscribing to relation changes.

Do not turn `locus` into a second D-Bus or a general graph mirror. It is only
the cross-service association store.

## Existing Similar Systems

GNOME TinySPARQL/Tracker is a low-footprint RDF triple store with SPARQL 1.1
and D-Bus endpoints. KDE/NEPOMUK explored a broader semantic-desktop relation
model and was later replaced in KDE by more focused indexing/search systems.
These are relevant prior art, but Rsynapse currently wants a much smaller,
typed session relation service rather than a general RDF desktop database.

## Boundaries

- Keep reusable shell framework code in `shell/core`.
- Keep Rsynapse product UI under `shell/`.
- Keep niri protocol projection in `niri-dbus`.
- Keep cross-service links in `locus`.
- Keep agent state in AgentDBus/adjacent agent service repos.
- Keep project/note state in remarked-related repos unless a shared project
  service is intentionally introduced.

## Implementation Rules

- Prefer `zbus` for D-Bus services and clients.
- Prefer typed structs/enums at service boundaries.
- Expose `ObjectManager`, `Properties`, and explicit signals where they fit.
- Use stable IDs for persistent associations; do not persist short-lived object
  paths unless the producing service guarantees their stability.
- Avoid polling, sleeps, debounce-based correctness, and FUSE for live shell
  state.
- Keep generated or protocol-derived code generated; do not patch generated
  files manually.
