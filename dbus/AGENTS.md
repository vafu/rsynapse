# D-Bus Services

Service implementations and shared contracts.

- `locus/`: cross-service relation store and `locus` CLI.
- `projd/`: project-management service, model/API crate, persistent store, and
  `proj` CLI. It must remain independent of shell UI, locus, steward, and
  compositor protocols.
- `workspace-dbus/`: compositor-neutral workspace/window/output projection.
  `api/` is the consumer contract; `service/` owns Niri/Umbriel backends.
- `workspace-policy/`: shared workspace naming/project/visibility policy used by
  workspace consumers.

Keep typed contracts and explicit signals. Service state is authoritative;
cross-service associations belong in locus unless an existing domain exception
is documented.
