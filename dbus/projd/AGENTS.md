# Projd

Project-management service, persisted store, and `proj` CLI.

- Owns projects, checkouts, goals, labels, work history, and active-goal state.
- Must not depend on shell, compositor, locus, steward, AgentDBus, Graphite, or
  Grafana.
- Each checkout/worktree is a project with stored name, absolute CWD, icon, and
  labels.
- Preserve stable project and task-instance IDs across key reuse/deletion.
- Expose reactive ObjectManager/property APIs; keep storage in SQLite.
- App clients consume the public model/API through `apps/shared/data-sources`.
