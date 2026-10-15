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
    opens a searchable catalog with a tile and direct remove icon for each
    project. Each checkout/worktree is a separate project. Clicking a tile assigns it;
   the directory picker is inside this selector popup.
   The current row's edit icon opens a text field with apply/cancel controls.
   Remove permanently deletes project metadata from projd. Workspace-only
   unassignment lives in the Workspace view.
   The selector stays open while removing entries, including the currently
   assigned project, so multiple registrations can be cleaned up in one pass.
    Remove applies to the project and its optional checkout; the same
   action is available on the current project and catalog entries.
   Project files and directories are untouched. Unassigning restores the saved workspace name and disables
   automatic reassociation until an explicit project assignment.
   The editor's "Keep after restart" checkbox reads and sets explicit relation
   persistence; new names start session-only. Icon overrides explicitly opt
   into persistence. `locus persist ... true|false` can toggle an
    existing relation without replacing its contents (see `../locus/README.md`).
    Associated icons subscribe to the project's initial/live `Icon` property;
    changing the association switches subscriptions. Only unset icons use shell
    heuristics, persisted conditionally so manual selections always win. Project
    names/icons persist in projd independently for each worktree. Unassociated
    workspace icon overrides remain persistent locus relations.
   Automatic titles use the stored `Project.Name`, or `empty` without a
  project; preferred names are preserved by steward.
  Unassigned workspace buttons have a leading folder-plus icon. **Assign project**
  selects a directory to register with projd and associate through steward.
  The popup and picker target the workspace clicked, even if focus changes.
  `rsynapse-shell request project-init` opens the same picker for the focused
   workspace and can be used as a compositor shortcut command.
   The bar also shows the project's active goal as a button (or **Choose goal**).
   The button shows only the current subgoal title when one is being worked on,
   otherwise the main goal title. Parent context stays in the tooltip/checklist.
   A selected parent follows its unique in-progress child; multiple in-progress
   siblings require an explicit selection instead of guessing the current step.
   The button is hidden when the current project has no goals.
   The main bar uses a CenterBox: windows on the left, a context block in the
   middle, and status widgets on the right. The context block is itself a
   CenterBox, with project/workspace title plus Git information in its center
   slot. Its start slot expands to fill the left section, with the goal
   right-aligned inside that section, next to the centered project/Git row.
   The goal is an unmeasured overlay within the left section, so long
   titles shrink to available space instead of displacing the center row.
   It opens a nested checklist popover: checkboxes complete/reopen tasks, and clicking a task title
   opens its Markdown note in `$EDITOR` (then `$VISUAL`, otherwise `vi`) through a
   terminal. The note path comes directly from projd's goal snapshot; opening a
   note does not invoke a `proj` executable from `PATH`. Titles/descriptions
   and active selection follow projd properties. Task lists come from its reactive
   snapshots; the shell does not watch or parse Markdown for checklist state.
   A disclosure control at the right end of each parent row expands/collapses its
   subgoal group; leaf rows have no trailing control. Active tasks are selected
   with `proj goal start` or `proj goal active`. Completed groups fold
   automatically, and completed titles are struck through. A group whose children
   are all complete is presented as complete without changing its stored status;
   its checkbox stays mixed until explicitly completed. Manual expansion survives
   updates; completion/reopening transitions fold/expand the group. Siblings sort
   unfinished first, then high-to-low priority, then oldest creation date; unknown
   legacy dates sort last within their priority. Children stay under their parent.

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
