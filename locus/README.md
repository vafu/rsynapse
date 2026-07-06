# locus

`locus` is a small session D-Bus service for storing relations between objects
exposed by other desktop services.

The role is narrow:

- Store typed associations such as workspace -> project or window -> agent.
- Refer to external objects through D-Bus object references or stable typed
  keys.
- Emit change signals so shell clients can reactively resolve associations.
- Avoid mirroring source-service properties or becoming a replacement object
  bus.

Adjacent technologies such as TinySPARQL/Tracker provide RDF stores and D-Bus
endpoints, but this project is deliberately a small desktop relation service
rather than a general RDF/SPARQL database.

## Current Surface

- Owns `org.rsynapse.Locus` on the session bus.
- Exports `/org/rsynapse/Locus` with `org.rsynapse.Locus.Relations1`.
- Supports `Set`, `Unset`, `Clear`, `Targets`, `Subjects`, and `List`.
- Emits relation signals after persistence succeeds.
- Persists records atomically to `$LOCUS_RELATIONS_PATH` or
  `$XDG_STATE_HOME/rsynapse/locus/relations.json`.

## Commands

From this directory:

```sh
cargo test
cargo run
busctl --user introspect org.rsynapse.Locus /org/rsynapse/Locus
```

From the repository root:

```sh
cargo test --manifest-path locus/Cargo.toml
```
