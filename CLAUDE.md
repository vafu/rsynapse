# rsynapse

Modular application launcher daemon for Linux, written in Rust. Operates as a headless backend that loads plugins as shared libraries (`.so`) and exposes search/execute functionality over D-Bus. Any UI client can connect to it.

**Status**: Work in progress. API, plugin interface, and installation procedures are unstable.

## Project structure

Rust workspace with `resolver = "3"`.

| Crate | Type | Purpose |
|---|---|---|
| `rsynapse-daemon` | Binary | Core daemon. Loads plugins, serves D-Bus API |
| `rsynapse-cli` | Binary | CLI client for testing (search/exec via D-Bus) |
| `rsynapse-plugin` | Library | `Plugin` trait and `ResultItem` struct |
| `rsynapse-plugin-launcher` | cdylib | Indexes `.desktop` files, fuzzy search via `skim` |
| `rsynapse-plugin-shell` | cdylib | Validates shell commands with `sh -n`, low priority score |
| `rsynapse-plugin-calc` | cdylib | Evaluates math expressions via `meval`, score 100.0 |
| `rsynapse-plugin-commands` | cdylib | Generic command runner. Matches queries against regex patterns from `~/.config/rsynapse/commands.toml`, runs shell commands, expects JSON/JSONL output |

## Architecture

- **Plugin loading**: Daemon scans a directory for `.so` files, calls the `_rsynapse_init` FFI entry point to get a `Box<dyn Plugin>`. Debug builds load from `./target/debug/`, release from `~/.local/lib/rsynapse/plugins/`.
- **Search flow**: Query fans out to all plugins -> each returns `Vec<ResultItem>` with scores -> daemon sorts by score descending -> converts to `DbusResultItem` and returns.
- **D-Bus interface**: `org.rsynapse.Engine1` on service `com.rsynapse.Engine`, object path `/org/rsynapse/Engine1`. Methods: `Search(query) -> Vec<(id, title, description, icon, command)>`, `Execute(id)` (WIP).
- **Plugin trait**: `name() -> &'static str` and `query(&str) -> Vec<ResultItem>`. Plugins must be `Send + Sync`.

## Writing a new plugin

1. Create a new crate in the workspace with `crate-type = ["cdylib"]`
2. Depend on `rsynapse-plugin`
3. Implement the `Plugin` trait
4. Export the FFI entry point:
   ```rust
   #[unsafe(no_mangle)]
   pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
       Box::into_raw(Box::new(MyPlugin))
   }
   ```
5. Add the crate to the workspace `members` in the root `Cargo.toml`
6. Install the plugin (see below)

## Build & run

```bash
cargo build                          # debug build (plugins in ./target/debug/)
cargo build --release                # release build
cargo run -p rsynapse-daemon         # run daemon (debug)
cargo run -p rsynapse-cli -- search firefox   # test search
```

## Installing plugins

Release builds load plugins from `~/.local/lib/rsynapse/plugins/`. Plugins are symlinked there so they stay in sync with `cargo build --release`:

```bash
cargo build --release
ln -sf "$(pwd)/target/release/librsynapse_plugin_foo.so" ~/.local/lib/rsynapse/plugins/
```

After building a new plugin, always symlink it. The daemon must be restarted to pick up new plugins.

## Key dependencies

- `tokio` — async runtime
- `zbus` — D-Bus communication
- `libloading` — dynamic `.so` loading
- `fuzzy-matcher` (skim) — fuzzy matching in launcher plugin
- `freedesktop-desktop-entry` — `.desktop` file parsing
- `notify` — filesystem watcher for app reindexing
- `meval` — math expression evaluation
- `clap` — CLI argument parsing
- `tabled` — table output formatting

## Known WIP / TODOs

- `Execute` method on the daemon is scaffolded but not implemented
- Command history plugin mentioned in README but not yet created
- CLI `exec` subcommand prints a message but doesn't do anything yet
- CLI `main.rs` has dead code (unused `vec` + `split_at_mut` block)
