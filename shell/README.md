# Rsynapse Shell

`shell/` is the Rsynapse shell UI monorepo. It contains reusable Rust
GTK4/Relm4 framework crates plus concrete Rsynapse UI surfaces.

## Layout

- `core/shell-core`
  Generic app startup, stylesheet loading, layer-shell window setup, and
  Observable source primitives.

- `core/background-effect`
  Reusable GTK4 `ext-background-effect-v1` helpers.

- `core/macros`
  Relm4 model/source binding procedural macros.

- `core/rx-macros`
  Small RxRust composition macros.

- `app`
  The current combined `rsynapse-shell` package. It owns the bar, OSD,
  notifications bridge, request socket, styles, and Rsynapse-specific UI
  policy.

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
