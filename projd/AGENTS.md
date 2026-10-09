# Projd project-management boundary

`projd` is a reusable D-Bus service with its own persistent store. `proj` is its
CLI client. Neither daemon nor CLI depends on shell-source, niri, steward,
locus, AgentDBus, Graphite or Grafana. Agents use the public CLI/API as clients.

Own project identities, optional Git checkouts/worktrees, metadata, goals and
project-management workflows here. Each checkout/worktree is its own project;
non-Git projects have no checkout. Projects own their stored name, absolute CWD
and icon. Automatic resolution must not overwrite CWD or name with the focused
window's state. There are no Context objects or relative-CWD descriptors.
Checkout.Project is a derived reverse object-path reference, not stored ownership.
Expose ObjectManager and reactive object properties. Keep stable IDs in storage.

Steward owns desktop integration and association policy; locusd stores/queries
the resulting cross-service associations. UIs and integration clients compose
services through shell-source. Do not add workspace/window APIs to this service.

Grafana belongs to shared observability deployment. Its HTTP adapter is a client
of this D-Bus API, with no project-management persistence or business ownership.
Metrics and side-effect handlers belong to steward. No composite score is set.

Use public crates.io builds from a neutral CWD and the existing isolated Cargo
cache. Test persistence, stable identity, property changes and CLI/API parity.
