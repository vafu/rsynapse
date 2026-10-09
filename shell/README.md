# Rsynapse Shell

`shell/` is the Rsynapse shell UI monorepo. It contains reusable Rust
GTK4/Relm4 framework crates plus concrete Rsynapse UI surfaces.

## Layout

- `core/shell-core`
  Generic app startup, stylesheet loading, and layer-shell window setup.
  Re-exports the Observable source primitives from `core/shell-source`.

- `core/shell-source`
  UI-free Observable source primitives (D-Bus properties, signals,
  ObjectManager snapshots, Rx composition helpers) for widgets and
  headless consumers alike.

- `core/macros`
  Relm4 model/source binding procedural macros.

- `core/rx-macros`
  Small RxRust composition macros.

- `widgets/nerd-icon-picker`
  Reusable `pick-icon`-backed fuzzy GTK picker for Nerd Font glyphs and
  consumer-supplied specific icon rows.

- `app`
  The current combined `rsynapse-shell` package. It owns the bar, OSD,
  notifications bridge, request socket, styles, and Rsynapse-specific UI
  policy.
   The workspace icon and title use Adwaita ButtonContent to open a Workspace
   popup. An Adwaita InlineViewSwitcher switches between **Workspace** name
   editing and **Project** selection, defaulting to the current association.
   The Project view separates the current association from the project catalog.
   The current-project row has direct remove and inline name-editing icons. **Choose project**
   opens a searchable catalog grouped by project, with checkout rows and a
   remove and selection checkmark buttons on project entries. A checkmark assigns
   the checkout; the directory picker is inside this selector popup.
   The current row's edit icon opens a text field with apply/cancel controls.
   Remove permanently deletes project metadata from projd. Workspace-only
   unassignment lives in the Workspace view.
   Remove applies to the project and its registered checkouts/contexts; the same
   action is available on the current project and catalog entries.
   Project files and directories are untouched. Unassigning restores the saved workspace name and disables
   automatic reassociation until an explicit project assignment.
   The editor's "Keep after restart" checkbox reads and sets explicit relation
   persistence; new names start session-only. Icon overrides explicitly opt
   into persistence. `locus persist ... true|false` can toggle an
   existing relation without replacing its contents (see `../locus/README.md`).
  Automatic titles use the project's cwd label, or `empty` without a
  project; preferred names are preserved by steward.
  Unassigned workspace buttons have a leading folder-plus icon. **Assign project**
  selects a directory to register with projd and associate through steward.
  The popup and picker target the workspace clicked, even if focus changes.
  `rsynapse-shell request project-init` opens the same picker for the focused
  workspace and can be used as a compositor shortcut command.

- `launcher`
  The launcher workspace. It owns the D-Bus launcher daemon, CLI, GTK launcher
  UI, plugin API, and bundled plugins.

## Architecture

Shell UI state is D-Bus-first:

```text
D-Bus services -> zbus streams -> shell_core::source::Observable<T> -> Relm4
```

Framework crates must stay generic. Product behavior, widget view models,
styling, request commands, and launcher policy belong in consumer crates.

## Common Commands

From this directory:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo test --workspace
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo fmt --check
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-shell
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-notifications
```

Launch either UI with the GTK inspector opened at startup by passing `inspect`:

```sh
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-shell -- inspect
env CARGO_TARGET_DIR=/tmp/rsynapse-shell-target cargo run -p rsynapse-shell --bin rsynapse-notifications -- inspect
```

Stop the corresponding systemd user service first when it is already running;
GTK applications use a single instance per application ID.

The launcher is a nested workspace:

```sh
cd launcher
cargo test --workspace
cargo run -p rsynapse-daemon
cargo run -p rsynapse-cli -- search firefox
```

## More Detail

- `PROJECT.md` describes the shell framework design and constraints.
- `PLAN.md` tracks the live roadmap.
- `SOURCE_API.md` describes the Observable-first source API.
- `AGS_REFERENCE.md` records product behavior to preserve from the old AGS
  shell without treating that implementation as an architecture template.
