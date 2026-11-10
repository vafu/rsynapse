# Rsynapse Launcher Workspace

This directory contains the launcher portion of Rsynapse. It is part of the
root Cargo workspace.

The launcher consists of a headless D-Bus daemon, a CLI, a GTK launcher UI, a
dynamic plugin API, and bundled plugins.

## Crates

| Crate | Type | Purpose |
| --- | --- | --- |
| `rsynapse-daemon` | Binary | Owns `org.rsynapse.Engine`, loads plugins, serves search/execute over D-Bus. |
| `rsynapse-cli` | Binary | CLI client for querying and executing launcher results. |
| `rsynapse-ui` | Binary | GTK4/Relm4 launcher UI client. |
| `rsynapse-plugin` | Library | Plugin trait, routing spec, query/key context, and result item types. |
| `rsynapse-launcher-api` | Library | Shared D-Bus wire types and generated engine/toggle proxies. |
| `rsynapse-plugin-launcher` | `cdylib` | Indexes `.desktop` applications from XDG application directories. |
| `rsynapse-plugin-shell` | `cdylib` | Offers syntactically valid shell commands as low-priority results. |
| `rsynapse-plugin-calc` | `cdylib` | Evaluates calculator expressions with `meval`. |
| `rsynapse-plugin-commands` | `cdylib` | Runs configured command queries from `~/.config/rsynapse/config.toml`. |
| `rsynapse-plugin-audio` | Library | Selects the default PipeWire output. |
| `rsynapse-plugin-bluetooth` | Library | Connects or disconnects Bluetooth devices. |
| `rsynapse-plugin-mpris` | Library | Selects which MPRIS player the shell media controls target. |

## D-Bus API

The daemon owns:

- Service: `org.rsynapse.Engine`
- Object path: `/org/rsynapse/Engine1`
- Interface: `org.rsynapse.Engine1`

Methods:

- `Search(request: SearchRequest) -> SearchResponse`
- `Describe() -> DescribeResponse`
- `HandleKey(request: KeyRequest) -> KeyResponse`
- `Execute(id: String) -> String`

`SearchRequest` carries invocation params, the query, and the current selection.
The first param selects a direct-callable plugin by id/alias; remaining params
and the query are forwarded to that plugin. If no plugin matches, plugins with
`MainOnly`/`Both` scope are queried and all params are forwarded as filters.
`SearchResponse` includes the focused plugin id/name, optional live summary,
semantic action hints, and result rows. Launcher chrome is UI-owned.

`Describe` returns plugin routing metadata and aliases.

`HandleKey` forwards non-navigation keys (for example `Delete`) to the plugin
that owns the selected result. Plugins can perform an action directly and return
`refresh`, `close`, and `message` flags.

`Execute` uses the daemon's cached result list from the latest search. Plugins
can handle execution directly, provide a default execute template, or users can
override plugin execute templates in `~/.config/rsynapse/config.toml`.

Plugins declare one `PluginSpec`: stable id, name, aliases, and scope
(`DirectOnly`, `MainOnly`, or `Both`). Built-in direct entry points are `apps`
(aliases: app, applications, launch), `projects` (project, proj), `workspaces`
(workspace, ws), `shell` (sh, run, command), `calc` (calculator, =), `commands`
(cmd, command), `audio`, `bluetooth`, and `mpris` (media, player, music). Selecting an
MPRIS player persists through Locus and the shell follows it reactively; choose
`Automatic` to revert to following the playing player.

The UI accepts positional launcher params:

```sh
rsynapse-ui
rsynapse-ui workspaces
rsynapse-ui workspaces --hold
rsynapse-ui apps firefox
```

`--hold` opens the launcher for modifier-held cycling: Tab/Shift+Tab move
the selection while the launcher keeps focus, and releasing Mod or Alt applies
the selection. Bind it in the compositor with `repeat = false`, for example
`Mod+Tab`.

For the workspace group, `--current-output` (or the `current-output` param)
limits results to the focused output. Omit it to list workspaces on every
enabled output:

```sh
rsynapse-ui workspaces --hold --current-output
rsynapse-ui workspaces --hold
```


## Build And Run

For debug runs, start from this directory so the daemon finds debug plugins
under `./target/debug`:

```sh
cargo build --workspace
cargo run -p rsynapse-daemon
cargo run -p rsynapse-cli -- search firefox
cargo run -p rsynapse-cli -- search --param workspaces
cargo run -p rsynapse-cli -- describe
cargo run -p rsynapse-ui -- workspaces
```

From the repository root:

```sh
cargo test -p rsynapse-daemon -p rsynapse-cli -p rsynapse-ui
cargo build --release -p rsynapse-daemon -p rsynapse-cli -p rsynapse-ui
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
cargo install --path apps/launcher/rsynapse-daemon --locked --force --root ~/.local
cargo install --path apps/launcher/rsynapse-cli --locked --force --root ~/.local
cargo install --path apps/launcher/rsynapse-ui --locked --force --root ~/.local
```

Manual plugin install:

```sh
cargo build --release \
  -p rsynapse-plugin-launcher \
  -p rsynapse-plugin-shell \
  -p rsynapse-plugin-calc \
  -p rsynapse-plugin-commands

mkdir -p ~/.local/lib/rsynapse/plugins
install -m 0755 target/release/librsynapse_plugin_*.so \
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
