# Rsynapse Local Install

This directory owns user-local install artifacts for Rsynapse.

Run from the repository root:

```sh
./install/local.sh
```

The installer writes only user-local paths by default:

- Binaries: `~/.local/bin`
- Launcher plugins: `~/.local/lib/rsynapse/plugins`
- D-Bus activation files: `~/.local/share/dbus-1/services`
- systemd user units: `~/.config/systemd/user`

Installed binaries currently include:

- `locus`
- `niri-dbus`
- `rsynapse-shell`
- `rsynapse-notifications`
- `rsynapse-daemon`
- `rsynapse-cli`
- `rsynapse-ui`
- `proj`

Installed D-Bus activation files currently include:

- `org.rsynapse.Engine.service`
- `org.rsynapse.Locus.service`
- `org.rsynapse.Niri.service`

Installed systemd user units currently include:

- `rsynapse-shell.service`
- `rsynapse-notifications.service`

Set `PREFIX=/path` to install binaries, plugins, and D-Bus activation files
under a different prefix. systemd user units are always installed under
`~/.config/systemd/user`.

The script also removes older Rsynapse service names that predate the current
`org.rsynapse.*` naming and the combined shell process layout.
