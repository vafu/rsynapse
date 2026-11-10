# Locus

Session relation store and `locus` CLI.

- Stores typed associations between externally owned objects; it does not become
  a general graph mirror or project database.
- Source services remain authoritative for their own properties.
- Support set/unset/query/watch and durable relation persistence.
- App consumers go through `data-sources`, not direct zbus proxies.
