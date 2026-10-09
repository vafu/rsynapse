# Rsynapse relation service (`org.rsynapse.Locus`)

`locusd` is a small session D-Bus service for storing relations between objects
exposed by other desktop services.

This directory is the source for both `locusd` (daemon) and `locus` (CLI).
The public Rust crate remains named `locus`, and the D-Bus service/interface
names are unchanged. Cargo explicitly defines these two binaries; plain
`cargo run` starts `locusd`. The former daemon binary named `locus` is replaced
by the CLI when deploying this version.

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
- Supports `Set`, `Unset`, `Clear`, `ClearSubject`, `Targets`, `Subjects`, and `List`.
- Uses typed relation endpoints for subjects and targets:
  `StableKey { kind, id }` or
  `DBusObject { bus, service, path, interface }`.
- Emits relation signals after required persistence succeeds.
- `Clear` emits each removed record and then a coarse completion signal.
- Persists explicitly durable records atomically to `$LOCUS_RELATIONS_PATH` or
  `$XDG_STATE_HOME/rsynapse/locus/relations.json`.
- Durable workspace preferences such as icon overrides should use the named
  workspace key `org.rsynapse.niri.workspace.name`; numeric workspace IDs are
  suitable only as a compatibility fallback for unnamed workspaces.

## Explicit Persistence Contract

Persistence belongs to each relation record, never to an endpoint kind. New
records default to **`persist: false`**. Session-only records stay available to
all reads/signals until removal or service exit. `persist: true` opts a record
into the disk snapshot, including records involving window IDs or D-Bus paths.

Existing disk records missing the JSON `persist` field migrate as **true**,
including window relations previously excluded by hardcoded policy. Supported
legacy string endpoints are decoded automatically. No deletion is needed.
Snapshots contain only records with `persist: true`, as flat JSON objects with
the original record fields plus `persist`. Explicit false entries on disk are
discarded on open and the snapshot is cleaned.

Bus: `org.rsynapse.Locus`; path: `/org/rsynapse/Locus`;
interface: `org.rsynapse.Locus.Relations1`.

The original `RelationRecord` and existing method/signal wire signatures are
preserved. New methods return `RelationState { record, persist: bool }`:

| Member | Input signature | Output signature |
| --- | --- | --- |
| `SetWithPersistence` | `a{ss}sa{ss}a{ss}b` | `((a{ss}sa{ss}a{ss}tt)b)` |
| `SetOneWithPersistence` | `a{ss}sa{ss}a{ss}b` | `((a{ss}sa{ss}a{ss}tt)b)` |
| `SetPersistence` | `a{ss}sa{ss}b` | `((a{ss}sa{ss}a{ss}tt)b)` |
| `ListWithPersistence` | `s` | `a((a{ss}sa{ss}a{ss}tt)b)` |
| `PersistenceChanged` (signal) | `((a{ss}sa{ss}a{ss}tt)b)` | — |

Set inputs are subject, relation, target, metadata, persist. `SetPersistence`
takes subject, relation, target, persist and requires an existing record;
otherwise it returns `org.freedesktop.DBus.Error.FileNotFound`. Toggling does
not replace metadata or change timestamps. `ListWithPersistence("")` lists all
records; a nonempty string filters by relation. Explicit setters emit the
existing added/updated/removed signals plus `PersistenceChanged`; toggles emit
`PersistenceChanged`. Subscribe before reading the initial state.

Legacy `Set`/`SetOne` create session-only records and preserve an existing
matching record's persistence flag on updates. A replacement target is a new
record and therefore defaults to false. These legacy writes emit their original
relation signals; consumers of persistence-aware state should refresh on those
signals as well as `PersistenceChanged`.

Enabling persistence writes and fsyncs the snapshot before acknowledging;
disabling rewrites it without the record while retaining the in-memory record.
Writes use a temporary file, atomic rename, and directory fsync. A failed write
before rename leaves in-memory state unchanged. Restarts restore only durable
records. Typed proxy methods are generated from the trait in `src/lib.rs`.

### CLI

```sh
locus list                         # JSON states with boolean persist
locus set '{"type":"stable-key","kind":"example.subject","id":"1"}' \
  example.link '{"type":"stable-key","kind":"example.target","id":"2"}' \
  '{}' --persist true
locus persist '{"type":"stable-key","kind":"example.subject","id":"1"}' \
  example.link '{"type":"stable-key","kind":"example.target","id":"2"}' false
```

`set` and `set-one` accept `--persist true|false` (default false), including
updates. `persist` toggles an existing record without replacing its payload.

Callers choose policy explicitly: steward's computed names and git snapshots
are session-only; workday summaries are durable. The shell name editor offers
"Keep after restart" and reads the current flag. Icon preferences explicitly
opt into durability; the CLI can toggle any existing preference.

Consumers storing durable settings/classifications/summaries should use
`SetOneWithPersistence(..., true)` and decode `RelationState`, rather than
assuming legacy `SetOne` persists new records. In particular, the dashboard
workday-target settings and steward productivity callers need that opt-in.
Goal storage is owned by the dedicated planner, not this persistence contract.

## Commands

From this directory:

```sh
cargo test
cargo run --bin locusd
cargo run --bin locus -- --help
busctl --user introspect org.rsynapse.Locus /org/rsynapse/Locus
```

From the repository root:

```sh
cargo test --manifest-path locus/Cargo.toml
```

The focused CLI/D-Bus integration check launches its own private bus and temp
snapshot (build both binaries first):

```sh
python3 locus/tests/persistence.py /path/to/target/debug
```

## Build And Coordinated Deployment

Run builds from `/tmp/opencode` with the workspace's public-crates Cargo home:

```sh
env CARGO_HOME=/home/vfuchedzhy/proj/rsynapse/steward/.cargo-home \
  CARGO_TARGET_DIR=/tmp/opencode/locus-persistence-release-target \
  cargo build --release --locked \
  --manifest-path /home/vfuchedzhy/proj/rsynapse/locus/Cargo.toml --bins
```

Outputs are `locus-persistence-release-target/release/locusd` and
`locus-persistence-release-target/release/locus` beneath `/tmp/opencode`.
`cargo install --path .../locus` also installs both explicitly declared bins;
the workspace's `install/local.sh` renders their activation/unit templates.

During a coordinated deployment, stop the current `org.rsynapse.Locus` owner
and its writers before replacing the old daemon executable named `locus`.
Install both release binaries under `~/.local/bin`. Render
`install/dbus-1/services/org.rsynapse.Locus.service.in` into
`~/.local/share/dbus-1/services/org.rsynapse.Locus.service` and
`install/systemd/user/locusd.service.in` into
`~/.config/systemd/user/locusd.service`, substituting `@LOCAL_BIN@` with the
absolute binary directory. Reload user systemd and start `locusd.service`,
then resume updated consumers. Activation executes `locusd` and delegates to
that same `Type=dbus`, `BusName=org.rsynapse.Locus` unit.

An existing unit running `~/.cargo/bin/locusd --schema ...` must be replaced by
the rendered unit. This implementation takes no arguments: configure the
snapshot through `LOCUS_RELATIONS_PATH` or the existing XDG state default;
`--schema`/`--static-store` are not supported. Preserve the existing snapshot
and any configured store-path override. `locusd --help` describes its role;
`locus --help` lists CLI commands without connecting to D-Bus.

Dashboard integration helpers must set `LOCUS_BIN` to the daemon, for example
`/tmp/opencode/locus-persistence-target/debug/locusd` for private-bus tests or
`~/.local/bin/locusd` after deployment. Goal storage remains in the separate
SQLite planner with its typed Planner D-Bus projection.

No live daemon is restarted by the persistence tests.
