# Milestone 00: Workspace Foundation

## Goal

Establish `~/proj/rsynapse` as the product workspace for the Rsynapse desktop
stack, with clear ownership boundaries and an explicit architecture direction:

```text
D-Bus services -> zbus streams -> shell-core Observables -> Relm4
```

This milestone prepares the codebase for DBus-backed live shell state.

## Scope

- Document ownership for `shell/`, `niri-dbus/`, `locus/`, and root workspace
  guidance. `shell/` is the UI monorepo, including reusable shell framework
  crates and concrete UI surfaces.
- Decide whether `~/proj/rsynapse` is a single Cargo workspace or a coordinated
  multi-project root.
- Capture verification commands for every Rust project in the directory.
- Identify current shell hot paths that still need DBus-backed replacements.
- Move reusable framework code from `~/proj/locus-shell` into
  `~/proj/rsynapse/shell/core`.

## Non-Scope

- Migrating widgets to D-Bus.
- Implementing new D-Bus services.
- Rewriting Relm4 components or source macros.
- Splitting the current combined `rsynapse-shell` app into separate `bar/`,
  `osd/`, and `notifications/` crates.
- Renaming launcher packages or binaries such as `rsynapse-daemon`; those can
  become launcher-scoped names later.
- Reintroducing provider runtimes, FUSE-first live state, or a second source
  abstraction beside `shell-core` Observables.

## Deliverables

- Root workspace docs that explain each top-level project.
- A documented `shell/` layout where all GTK/Relm4 UI components live together:
  reusable framework crates under `shell/core`, the current combined shell app
  under `shell/app`, and the launcher workspace under `shell/launcher`.
- Verification commands for the coordinated workspace.
- A short source migration inventory kept outside the active docs as historical reference after the backend removal.
- Updated guidance that says D-Bus is the target live-state backend.
- Stale docs marked or updated where they describe filesystem reads as the target shell hot path.

## Implementation Steps

1. Inventory top-level directories and record each project role.
2. Inspect project manifests and decide whether a root Cargo workspace is useful
   now or premature coupling.
3. Add or update root documentation with boundaries:
   - all desktop UI components belong under `shell/`;
   - GTK/Relm4/source primitives belong in `shell/core`;
   - the current combined bar/OSD/notification app belongs in `shell/app` until
     it is intentionally split into surface crates;
   - the launcher workspace belongs in `shell/launcher`, including its daemon,
     CLI, plugin, and UI crates until package renames are done intentionally;
   - service implementations belong in their owning service crates.
4. Record format, check, and test commands for each standalone Rust project.
5. Create the historical hot-path migration inventory.
6. Run the documented checks and record pre-existing failures separately.

## Verification

If the root remains a coordinated multi-project directory, verify each project
directly:

```sh
cargo fmt --check --manifest-path ~/proj/rsynapse/shell/Cargo.toml
cargo check --manifest-path ~/proj/rsynapse/shell/Cargo.toml
cargo fmt --check --manifest-path ~/proj/rsynapse/shell/launcher/Cargo.toml
cargo check --manifest-path ~/proj/rsynapse/shell/launcher/Cargo.toml
cargo fmt --check --manifest-path ~/proj/rsynapse/niri-dbus/Cargo.toml
cargo check --manifest-path ~/proj/rsynapse/niri-dbus/Cargo.toml
cargo fmt --check --manifest-path ~/proj/rsynapse/locus/Cargo.toml
cargo check --manifest-path ~/proj/rsynapse/locus/Cargo.toml
```

Also verify the shell monorepo workspace after the move:

```sh
cd ~/proj/rsynapse/shell
cargo fmt --check
cargo check --workspace
```

## Done Criteria

- A newcomer can identify what every top-level project owns.
- A newcomer can identify what every `shell/` subproject owns, including
  `core/`, `app/`, and `launcher/`.
- Verification commands are documented and runnable.
- The D-Bus source direction is consistent across root guidance and framework
  docs.
- Former filesystem hot paths are known and removed or scheduled for DBus replacements.
- No product policy is moved into generic framework crates.
