# locus

`locus` will be a small session D-Bus service for storing relations between
objects exposed by other desktop services.

The intended role is narrow:

- Store typed associations such as workspace -> project or window -> agent.
- Refer to external objects through D-Bus object references or stable typed
  keys.
- Emit change signals so shell clients can reactively resolve associations.
- Avoid mirroring source-service properties or becoming a replacement object
  bus.

Existing adjacent technologies such as TinySPARQL/Tracker provide RDF stores
and D-Bus endpoints, but this project is intended to be a deliberately small
desktop relation service rather than a general RDF/SPARQL database.

Current implementation status:

- owns `org.rsynapse.Locus` on the session bus;
- exports `/org/rsynapse/Locus` with
  `org.rsynapse.Locus.Relations1`;
- supports `Set`, `Unset`, `Clear`, `Targets`, `Subjects`, and `List`;
- emits relation signals after persistence succeeds;
- persists records atomically to
  `$LOCUS_RELATIONS_PATH` or `$XDG_STATE_HOME/rsynapse/locus/relations.json`.

Run:

```sh
cargo test
cargo run
busctl --user introspect org.rsynapse.Locus /org/rsynapse/Locus
```
