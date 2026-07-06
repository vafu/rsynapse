# rsynapse-cli

`rsynapse-cli` is a command-line client for the launcher daemon at
`org.rsynapse.Engine`.

Run from `shell/launcher`:

```sh
cargo run -p rsynapse-cli -- search firefox
cargo run -p rsynapse-cli -- exec firefox.desktop
```

Installed usage:

```sh
rsynapse-cli search firefox
rsynapse-cli exec firefox.desktop
```

`search` prints a table with result IDs. `exec` sends one of those IDs back to
the daemon. The daemon executes it only if the result is still present in the
daemon's latest cached search results and the plugin has an execute template.
