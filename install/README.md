# Rsynapse Local Install

This directory owns user-local install artifacts for Rsynapse.

The optional Grafana metrics and workday settings panels have a separate local install:

```sh
bash install/dashboard-api.sh
```

It installs `rsynapse-dashboard-api`, enables its systemd user unit, builds the
local Grafana panels, and recreates Grafana with those panels mounted. Goal records
stay in projd and are managed with `proj goal` and the shell checklist. See
`../misc/dashboard-api/README.md` for the API and UI workflow.
The same install builds and mounts the read-only goal metrics panel and provisions
its timeline/effort/completion dashboard, preserving existing `metrics` volumes.

Run from the repository root:

```sh
./install/local.sh
```

The installer writes only user-local paths by default:

- Binaries: `~/.local/bin`
- Desktop helper scripts: `~/.config/scripts`
- Launcher plugins: `~/.local/lib/rsynapse/plugins`
- D-Bus activation files: `~/.local/share/dbus-1/services`
- Desktop entries: `~/.local/share/applications`
- systemd user units: `~/.config/systemd/user`

Installed binaries currently include:

- `rsynapse-locusd` (daemon owning `org.rsynapse.Locus`)
- `locus` (CLI for `rsynapse-locusd`, including `--persist true|false` and existing-record toggles)
- `rsynapse-workspace-dbus` (Niri/Umbriel backends; automatic session selection)
- `workspace-terminal-id` (terminal-specific window discovery over the neutral API)
- `rsynapse-shell`
- `rsynapse-notifications`
- `rsynapse-daemon`
- `rsynapse-cli`
- `rsynapse-ui`
- `proj`
- `rsynapse-projd`
- `rsynapse-steward`

Daemon binaries and their systemd units use matching `rsynapse-*` names; their
public D-Bus names remain `org.rsynapse.*`. Short commands `workspace-dbus`,
`steward`, `locusd`, and `projd` are installed as relative compatibility symlinks.
The `proj` and `locus` command names remain the domain-facing CLIs.

`install/session-services.py` owns daemon artifact/metadata installation and
legacy-name migration. The full installer calls it after building the binaries.
It atomically publishes Projd/Steward artifacts, reverses the former
`rsynapse-steward -> steward` symlink, renders activation/unit templates, replaces
owned legacy unit files with aliases, and migrates their enabled target links.
Existing unit drop-in directories remain intact and apply through the aliases.
Locally edited base unit files are preserved with a migration error instead of
being overwritten. Running service processes are not restarted by installation.

For offline staging/verification with already-installed canonical binaries:

```sh
python3 install/session-services.py --prefix /tmp/rsynapse-prefix \
  --config-home /tmp/rsynapse-config --no-reload
python3 install/tests/test_service_names.py
```

Installed desktop helper scripts currently include:

- `rsynapse-open-url`
- `ghostty-project-window`
- `umbriel-workspace-new`

`umbriel-workspace-new above|below` focuses a blank workspace immediately before
or after the current one. It selects the trailing anonymous empty workspace and
moves it into position using Umbriel's CLI, following workspace IDs across index
changes. Existing windows stay on their workspaces. It is intended for dynamic
workspace inventories; an already empty anonymous current workspace is reused.
The inserted blank follows normal dynamic pruning when left unused.

Example Umbriel bindings:

```toml
"Mod+Alt+Up" = { action = "spawn:$HOME/.config/scripts/umbriel-workspace-new above", repeat = false }
"Mod+Alt+Down" = { action = "spawn:$HOME/.config/scripts/umbriel-workspace-new below", repeat = false }
"Mod+Alt+K" = { action = "spawn:$HOME/.config/scripts/umbriel-workspace-new above", repeat = false }
"Mod+Alt+J" = { action = "spawn:$HOME/.config/scripts/umbriel-workspace-new below", repeat = false }
"Mod+Alt+BackSpace" = { action = "workspace-focus-last", repeat = false }
"Mod+Shift+M" = { action = "spawn:$HOME/.local/bin/rsynapse-shell request workspace-selector toggle", repeat = false }
```

The Backspace binding returns to the previous workspace. An unused inserted
anonymous workspace is pruned after leaving it; the compositor retains its
automatic edge blanks. This native action does not launch a helper or create
another application-activation reservation. Named/static workspaces remain part
of their configured inventory. A workspace with windows remains intact.

The isolated integration check requires Umbriel and Python GI/GTK4:

```sh
dbus-run-session -- python3 install/tests/test_umbriel_workspace_new.py
```

`ghostty-project-window` opens a new Ghostty window in the selected workspace's
project CWD, resolved through workspace-dbus, Locus and projd. It supports both
Niri and Umbriel, keeps Ghostty's inherited-directory behavior disabled, and
falls back to `/tmp` when no valid project directory is available. Bind it to
the compositor's terminal shortcut, for example Umbriel's
`spawn:$HOME/.config/scripts/ghostty-project-window`. It is also linked from the
local binary directory.

Verify the launch helper without opening desktop windows:

```sh
python3 install/tests/test_ghostty_project_window.py
```

Agent-provider aliases, router lanes, and their hook configuration belong in
the user's dotfiles or the owning agent integration project.

The URL helper is also linked from the local binary directory. It detects Niri
or Umbriel from desktop identity (or an unambiguous socket); set
`RSYNAPSE_URL_COMPOSITOR=niri|umbriel` to override. Niri retains timestamp-based
MRU browser selection. Umbriel prefers an active matching browser, otherwise a
stable matching window on the workspace, since it exposes no focus history.

The installer does not install Git hooks or change Git configuration. Projd
watches Git metadata for registered checkouts directly; `proj update` is also
available for explicit refresh. Desktop association is `steward bind-current PATH`.

Older installations may have configured a global `core.hooksPath`. Inspect it
with `git config --global --get core.hooksPath`; if it is the former Rsynapse
directory (`~/.local/share/rsynapse/git-hooks`, or under an old `PREFIX`), remove
that setting with `git config --global --unset core.hooksPath`. Existing machine
configuration is not changed by this repository cleanup.

Installed D-Bus activation files currently include:

- `org.rsynapse.Engine.service`
- `org.rsynapse.Locus.service`
- `org.rsynapse.Workspace.service`
- `org.rsynapse.Proj.service`
- `org.rsynapse.Steward.service`

Installed desktop entries currently include:

- `rsynapse-open-url.desktop`, registered as the default `http` and `https`
  scheme handler.

Installed systemd user units currently include:

- `rsynapse-locusd.service` (`Type=dbus`, owning `org.rsynapse.Locus`)
- `rsynapse-projd.service` (`Type=dbus`, owning `org.rsynapse.Proj`)
- `rsynapse-workspace-dbus.service` (`Type=dbus`, owning `org.rsynapse.Workspace`, tied to the graphical session)
- `rsynapse-steward.service` (`Type=dbus`, owning `org.rsynapse.Steward`)
- `rsynapse-shell.service`
- `rsynapse-notifications.service`

Set `PREFIX=/path` to install binaries, plugins, and D-Bus activation files
under a different prefix. systemd user units are always installed under
`~/.config/systemd/user`.

Rust binaries are installed through normal `cargo install` calls. The caller's
Cargo configuration and `CARGO_TARGET_DIR` apply; projects do not share an
implicit cache or target under Steward. The optional `session-services.py
--install-built` mode reads release artifacts from each project's `target/`, or
from the caller's shared `CARGO_TARGET_DIR` when set.

Steward D-Bus activation delegates to `rsynapse-steward.service`; the fallback
Exec starts the same `rsynapse-steward` binary without systemd. Both its Associations1
and Session1 interfaces use this bus name. The installer enables the unit,
reloads systemd definitions and refreshes D-Bus activation configuration without
restarting running services. Systemd considers Steward ready when it owns the
bus name, after registering its interfaces.

Verify the activation fallback on a private D-Bus with isolated state:

```sh
python3 install/tests/test_steward_activation.py
```

This uses installed binaries, a disconnected private projection, and temporary
activation files. It starts no compositor and does not contact host systemd.

The workspace API cutover requires rebuilding Locus, workspace-dbus, steward
and shell together. Activation starts `rsynapse-workspace-dbus --backend auto` through
`rsynapse-workspace-dbus.service`; desktop identity takes precedence over inherited
compositor socket variables. The unit stops with `graphical-session.target`, so
the next session picks its own compositor after environment import.
It is ordered after, and requires an already-active, `graphical-session.target`.
This lets UWSM/native compositor readiness publish socket variables before the
service snapshots its environment; early D-Bus requests cannot start a permanently
disconnected projection. The pre-target alone is too early. Session activation
still starts the enabled unit automatically.
The installer removes the old `org.rsynapse.Niri` activation file and executable.
Steward owns workspace-relation cleanup. Project and goal storage is unchanged.

The startup-order regression uses uniquely named fixture units on the user
manager, without touching the real graphical session or its environment:

```sh
python3 install/tests/test_workspace_session_startup.py
```

The script also removes older Rsynapse service names that predate the current
`org.rsynapse.*` naming and the combined shell process layout.

The locus package explicitly builds and installs `rsynapse-locusd` and `locus`.
D-Bus activation starts `@LOCAL_BIN@/rsynapse-locusd` through `rsynapse-locusd.service`; the
rendered systemd unit uses the same executable with no arguments. Replace any
older unit invoking `~/.cargo/bin/locusd --schema ...` during a coordinated
deployment. Stop the current service owner and its writers before overwriting
the old daemon executable named `locus` with the CLI. The installer does not
restart `locusd` automatically. See `../dbus/locus/README.md` for neutral-directory
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
