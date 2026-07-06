# Rsynapse Launcher Workspace

This directory contains the launcher portion of Rsynapse. It is a nested Cargo
workspace under `shell/`, not the whole Rsynapse repository.

The launcher consists of a headless D-Bus daemon, a CLI, a GTK launcher UI, a
dynamic plugin API, and bundled plugins.

## Crates

| Crate | Type | Purpose |
| --- | --- | --- |
| `rsynapse-daemon` | Binary | Owns `org.rsynapse.Engine`, loads plugins, serves search/execute over D-Bus. |
| `rsynapse-cli` | Binary | CLI client for querying and executing launcher results. |
| `rsynapse-ui` | Binary | GTK4/Relm4 launcher UI client. |
| `rsynapse-plugin` | Library | Plugin trait and result item types. |
| `rsynapse-plugin-launcher` | `cdylib` | Indexes `.desktop` applications from XDG application directories. |
| `rsynapse-plugin-shell` | `cdylib` | Offers syntactically valid shell commands as low-priority results. |
| `rsynapse-plugin-calc` | `cdylib` | Evaluates calculator expressions with `meval`. |
| `rsynapse-plugin-commands` | `cdylib` | Runs configured command queries from `~/.config/rsynapse/config.toml`. |

## D-Bus API

The daemon owns:

- Service: `org.rsynapse.Engine`
- Object path: `/org/rsynapse/Engine1`
- Interface: `org.rsynapse.Engine1`

Methods:

- `Search(query: String) -> Vec<(id, title, description, icon, data)>`
- `Execute(id: String) -> String`

`Execute` uses the daemon's cached result list from the latest search. Plugins
can provide a default execute template, and users can override plugin execute
templates in `~/.config/rsynapse/config.toml`.

## Build And Run

For debug runs, start from this directory so the daemon finds debug plugins
under `./target/debug`:

```sh
cargo build --workspace
cargo run -p rsynapse-daemon
cargo run -p rsynapse-cli -- search firefox
cargo run -p rsynapse-ui
```

From the repository root, use the nested manifest explicitly:

```sh
cargo test --manifest-path shell/launcher/Cargo.toml --workspace
cargo build --manifest-path shell/launcher/Cargo.toml --workspace --release
```

## Install

Prefer the repository installer:

```sh
./install/local.sh
```

It installs release binaries under `~/.local/bin`, launcher plugins under
`~/.local/lib/rsynapse/plugins`, and the D-Bus activation file for
`org.rsynapse.Engine`.

Manual binary installs from the repository root:

```sh
cargo install --path shell/launcher/rsynapse-daemon --locked --force --root ~/.local
cargo install --path shell/launcher/rsynapse-cli --locked --force --root ~/.local
cargo install --path shell/launcher/rsynapse-ui --locked --force --root ~/.local
```

Manual plugin install:

```sh
cargo build --manifest-path shell/launcher/Cargo.toml --release \
  -p rsynapse-plugin-launcher \
  -p rsynapse-plugin-shell \
  -p rsynapse-plugin-calc \
  -p rsynapse-plugin-commands

mkdir -p ~/.local/lib/rsynapse/plugins
install -m 0755 shell/launcher/target/release/librsynapse_plugin_*.so \
  ~/.local/lib/rsynapse/plugins/
```

## Configuration

The daemon and command plugin read `~/.config/rsynapse/config.toml`.

Example execute override:

```toml
[plugins."Application Launcher"]
execute = "{data}"

[plugins."Shell Executor"]
execute = "{data}"
```

Example command plugin entry:

```toml
[[plugins.Commands.commands]]
pattern = "^note (.*)$"
command = "printf '{\"title\":\"Capture note\",\"data\":\"%s\"}\\n' '$1'"
```

Plugin templates can reference `{id}`, `{title}`, `{description}`, `{icon}`,
and `{data}`.
