# Rsynapse Local Install

This directory owns the local user-session install artifacts for Rsynapse.

Run:

```sh
./install/local.sh
```

The installer writes only user-local paths by default:

- binaries: `~/.local/bin`
- launcher plugins: `~/.local/lib/rsynapse/plugins`
- D-Bus activation files: `~/.local/share/dbus-1/services`
- systemd user units: `~/.config/systemd/user`

Set `PREFIX=/path` to install binaries, plugins, and D-Bus activation files
under a different prefix. Systemd user units are always installed under
`~/.config/systemd/user`.

