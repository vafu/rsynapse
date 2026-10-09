# Projd project-management boundary

`projd` is a reusable D-Bus service with its own persistent store. `proj` is its
CLI client. Neither daemon nor CLI depends on shell-source, niri, steward,
locus, AgentDBus, Graphite or Grafana. Agents use the public CLI/API as clients.

Own project identities, checkouts/worktrees, working contexts, metadata, goals
and project-management workflows here. A project can have many checkouts and
CWD contexts; never make one project's branch/CWD a last-focused-window value.
Expose ObjectManager and reactive object properties. Keep stable IDs in storage.

Steward owns desktop integration and association policy; locusd stores/queries
the resulting cross-service associations. UIs and integration clients compose
services through shell-source. Do not add workspace/window APIs to this service.

Grafana belongs to shared observability deployment. Its HTTP adapter is a client
of this D-Bus API, with no project-management persistence or business ownership.
Metrics and side-effect handlers belong to steward. No composite score is set.

Use public crates.io builds from a neutral CWD and the existing isolated Cargo
cache. Test persistence, stable identity, property changes and CLI/API parity.
