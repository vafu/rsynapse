# Tasks & Notes

A fresh Adwaita/Relm4 **layer-shell overlay** hosted by `rsynapse-shell`. This crate is a UI
component, not another application process. The primary shell controller owns one
window; buttons on every monitor route to that same controller.
The surface uses the overlay layer and `rsynapse-workspace` namespace, is centered
without reserving workspace space, and never enters normal tiling/window lists.
It requests keyboard focus while visible and releases it when hidden.

## Ownership

- Projd owns project/task identity, hierarchy, status, selection and history.
- Remarked owns notebook Markdown, frontmatter tags, links and knowledge workflows.
- Shell-source supplies the existing Observable/owner-change/subscription machinery.
- The UI owns navigation, filtering and drafts. Browsing a task does not select it
  as the project's active task; **Start** does that explicitly.

The sibling `~/proj/remarked` checkout supplies the `notebook-api` contract through
a relative path dependency. No notebook runtime, zk implementation, terminal
editor or text-generation model is linked into the shell.

## Use

Click the goal widget in the bar, press **Mod+N** in Umbriel, launch **Tasks & Notes**,
or run:

```sh
rsynapse-shell request workspace show
rsynapse-shell request workspace toggle
rsynapse-shell request workspace hide
```

The window opens in the focused project's context; a bar button captures its own
monitor's project. Choose projects/tasks in the sidebar, capture a note with
**Ctrl+N**, and save text and tags with **Ctrl+S**. **Escape**, **Ctrl+W**, or the
close button hides the overlay.
Unsaved drafts require save/discard before navigation or hiding. A notebook read
does not disable task actions, and stale/background responses cannot replace a
draft or a different note's editor. Undo history is reset at document boundaries.

Each project has a personal notebook at `<stored-project-cwd>/.notes/`, with zk
configuration/index under `.notes/.zk/`. The personal setup globally excludes
`.notes/` from Git, including projects registered below a repository root.

Many-to-many task membership lives in frontmatter `task/<immutable-instance-id>`
tags. Named task-key reuse does not inherit another instance's notes. There is no
primary-note record or Locus membership mirror. **Import task context** copies a
legacy proj context file into this notebook once, retaining the original file.

Tasks support planned, in-progress, completed, deferred and cancelled states.
Cancelled tasks retain their notes and appear under **Show finished tasks**.

The shell's project-aware CLI is `rnotes`:

```sh
rnotes task add --title 'Implement the feature'
rnotes --task TASK_KEY note capture '# Design finding'
rnotes --task TASK_KEY note list --json
rnotes note get NOTE_ID --body --format json
```

`--task` resolves the immutable instance through projd. The note service only sees
scope tags. Note body replacement is revision-guarded; conflicts retain the local
draft.

## Build and verification

GtkSourceView 5 development files are required, alongside GTK4/libadwaita. The
pinned native shell uses the caller's existing Rust toolchain:

```sh
nix develop .#native
cargo test --manifest-path shell/Cargo.toml -p rsynapse-workspace --lib
cargo test --manifest-path shell/Cargo.toml -p rsynapse-shell --lib request::tests
cargo build --manifest-path shell/Cargo.toml -p rsynapse-shell --release --bin rsynapse-shell
```

The ignored `workspace_ui_roundtrip` test runs the real layer-shell component against real
projd/Remarked processes on a private D-Bus. It covers source updates, task tags,
note loading/editing/saving/capture, draft navigation protection, CLI captures,
task mutation, layer role, keyboard focus/release and hiding. Build the Remarked
daemon/CLI, then run
`nix develop .#native --command bash shell/remarked/test-ui.sh`.
`PROJD_BIN`, `REMARKED_BIN` and `REMARKED_CLI_BIN` can override the defaults. The
wrapper starts a private headless Umbriel display and D-Bus, so keyboard-grab tests
do not interrupt the live desktop. Set `UMBRIEL_TEST_BIN` for a different Umbriel
binary, or explicitly opt into an existing test display with
`RSYNAPSE_UI_LIVE_DISPLAY=1`. Accessibility/portal isolation is applied before
starting the private bus, and cleanup detaches only test-owned portal mounts.

Synthesis, relation-discovery controls and rendered Markdown preview are outside
this initial core surface. Their backend contracts remain separate from managed
task state.
