# Rsynapse Local Install

This directory owns user-local install artifacts for Rsynapse.

The optional Grafana goal editor has a separate local install:

```sh
bash install/dashboard-api.sh
```

It installs `rsynapse-dashboard-api`, enables its systemd user unit, builds the
local Grafana panel, and recreates Grafana with that panel mounted. Goal records
stay in projd. See `../dashboard-api/README.md` for the API and UI workflow.

Run from the repository root:

```sh
./install/local.sh
```

The installer writes only user-local paths by default:

- Binaries: `~/.local/bin`
- URL helper scripts: `~/.config/scripts`
- Launcher plugins: `~/.local/lib/rsynapse/plugins`
- D-Bus activation files: `~/.local/share/dbus-1/services`
- Desktop entries: `~/.local/share/applications`
- systemd user units: `~/.config/systemd/user`
- git hooks: `~/.local/share/rsynapse/git-hooks`

Installed binaries currently include:

- `locusd` (daemon owning `org.rsynapse.Locus`)
- `locus` (CLI for `locusd`, including `--persist true|false` and existing-record toggles)
- `niri-dbus`
- `rsynapse-shell`
- `rsynapse-notifications`
- `rsynapse-daemon`
- `rsynapse-cli`
- `rsynapse-ui`
- `proj`
- `projd`
- `steward`

Installed URL helper scripts currently include:

- `rsynapse-open-url`

Installed git hooks currently include:

- `post-checkout`
- `post-merge`
- `post-rewrite`

When no global `core.hooksPath` is configured, the installer points it at the
Rsynapse hook directory so branch changes from any process refresh project
metadata through `proj update`, a D-Bus client of projd. Desktop workspace
association is now `steward bind-current PATH`. If another global hook path is already set, the
installer leaves it alone.

Installed D-Bus activation files currently include:

- `org.rsynapse.Engine.service`
- `org.rsynapse.Locus.service`
- `org.rsynapse.Niri.service`
- `org.rsynapse.Proj.service`

Installed desktop entries currently include:

- `rsynapse-open-url.desktop`, registered as the default `http` and `https`
  scheme handler.

Installed systemd user units currently include:

- `locusd.service` (`Type=dbus`, owning `org.rsynapse.Locus`)
- `projd.service` (`Type=dbus`, owning `org.rsynapse.Proj`)
- `rsynapse-shell.service`
- `rsynapse-notifications.service`

Set `PREFIX=/path` to install binaries, plugins, and D-Bus activation files
under a different prefix. systemd user units are always installed under
`~/.config/systemd/user`.

The script also removes older Rsynapse service names that predate the current
`org.rsynapse.*` naming and the combined shell process layout.

The locus package now explicitly builds and installs `locusd` and `locus`.
D-Bus activation starts `@LOCAL_BIN@/locusd` through `locusd.service`; the
rendered systemd unit uses the same executable with no arguments. Replace any
older unit invoking `~/.cargo/bin/locusd --schema ...` during a coordinated
deployment. Stop the current service owner and its writers before overwriting
the old daemon executable named `locus` with the CLI. The installer does not
restart `locusd` automatically. See `../locus/README.md` for neutral-directory
build commands, snapshot migration, and the deployment order.

After starting the new locusd and projd, run `python3 install/migrate-projd.py`
with steward and the dashboard API stopped. It backs up legacy records, verifies
the complete imported goal content before removing metadata mirrors, and keeps
existing workspace bindings and their persistence settings. Unavailable project
paths retain their legacy records; new automatic desktop bindings are session-only.
Only intentional workspace bindings are imported as projects. Standalone legacy
metadata-cache entries are backed up, but do not populate the project catalog.

The checkout-per-project model upgrade is handled automatically by projd when
opening an old SQLite store. It creates a `before-model-v2-*.sqlite3` backup,
splits grouped worktrees into projects, and preserves full goal history. Steward
remaps bindings and adopts workspace icons into projects. Deploy projd, proj,
steward, dashboard-api and shell together for the changed D-Bus record shapes.
