# Milestone 03: Locus Relation Service

## Goal

Implement `~/proj/rsynapse/locus` as a small session D-Bus service that stores
typed associations between objects or stable keys owned by other services.

Examples:

- niri workspace -> project
- niri window -> AgentDBus session
- project -> preferred terminal/session

The service stores relations. Source services remain authoritative for their
own objects and properties.

## Prior Art

TinySPARQL/Tracker provides a low-footprint RDF/SPARQL store with D-Bus
integration, and KDE/NEPOMUK explored a broader semantic-desktop relation model.
Those systems are useful context, but the initial Rsynapse need is narrower: a
typed session relation service, not a general RDF database.

## Scope

- Store typed relation records with subject, relation kind, target, metadata,
  and timestamps.
- Support both D-Bus object references and explicitly typed stable keys.
- Expose D-Bus methods for setting, clearing, and querying relations.
- Emit signals after successful relation changes.
- Persist local user/session state across service restarts.

## Non-Scope

- Shell UI policy.
- A filesystem-backed hot-path dependency.
- A generic graph database or schema/codegen system.
- Mirroring properties from upstream services.
- Owning upstream objects from other services.

## Reference Model

Use D-Bus object semantics directly when object paths are stable:

```rust
struct ObjectRef {
    bus: BusKind,
    service: String,
    path: OwnedObjectPath,
    interface: String,
}
```

Use stable typed keys when object paths are ephemeral or persistence matters:

```text
niri-workspace:<id>
niri-window:<id>
project:<id>
agent-session:<id>
```

Relation kinds should be typed names, preferably reverse-DNS strings:

```text
org.rsynapse.WorkspaceProject
org.rsynapse.WindowAgentSession
```

## D-Bus API Shape

Suggested service:

```text
Name:      org.rsynapse.Locus
Object:    /org/rsynapse/Locus
Interface: org.rsynapse.Locus.Relations1
```

Core methods:

```text
Set(subject, relation, target, metadata)
Unset(subject, relation, target)
Clear(subject, relation)
Targets(subject, relation) -> targets
Subjects(relation, target) -> subjects
List(relation) -> records
```

Signals:

```text
RelationAdded(subject, relation, target, metadata)
RelationRemoved(subject, relation, target)
RelationCleared(subject, relation)
```

The exact wire type for references should be chosen deliberately during
implementation. The API should avoid unstructured strings where a typed
reference can be represented cleanly, but it should not force D-Bus object paths
for persistent entities whose path is not stable.

## Implementation Steps

1. Add `zbus`, `zvariant`, serialization, tracing, and a small persistence
   backend.
2. Define relation DTOs, reference types, validation, and relation kind
   constants.
3. Choose initial persistence: JSON for inspectability or SQLite for stronger
   update semantics.
4. Implement atomic set, unset, clear, and query operations.
5. Emit signals only after storage commits.
6. Add CLI/dev commands for inspecting and editing relations if useful.
7. Add integration tests for D-Bus methods and emitted signals.

## Verification

- Unit-test reference validation, relation replacement, clearing, and persistence
  reload.
- Integration-test `Set -> Targets`, `Set -> signal`, and `Unset -> signal`.
- Restart the service and verify persisted workspace/project mappings remain.
- Exercise sample `workspace -> project` and `window -> agent` relations.
- Confirm shell consumers can subscribe through DBus services.

## Risks

- Key naming can drift between producing services; keep key construction
  centralized.
- Relation kind names can become product policy; start private and promote only
  when reused.
- Metadata can become untyped junk; keep v1 metadata small and documented.
- Signal ordering matters for UI correctness; emit after persistence commits.
- A too-general API can recreate the graph database we are trying to avoid.

## Done Criteria

- `locus` owns `org.rsynapse.Locus` on the session bus.
- Relations can be set, listed, cleared, persisted, and observed through D-Bus.
- Workspace/project and window/agent examples work without filesystem-backend reads.
- API docs are sufficient for `rsynapse-shell` source helpers.
- The service stores relations only; it does not mirror upstream object
  properties.
