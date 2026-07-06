# install Review

Status: reviewed

## Scope

Installation scripts, desktop integration templates, D-Bus activation, and
systemd user unit files.

## Findings

- **High - stale launcher plugins remain installed and active.** The installer
  copies current `librsynapse_plugin_*.so` files into the plugin directory at
  `install/local.sh:53`, but it does not remove older plugin `.so` files first.
  The daemon loads every `.so` in that directory, so renamed or removed plugins
  can keep running after an install.
- **Medium - template substitution is string-based and does not escape install
  paths.** `install_templates` injects `@LOCAL_BIN@` through `sed` at
  `install/local.sh:25`. Prefixes containing replacement-sensitive characters,
  or paths requiring systemd/D-Bus command-line escaping, can produce invalid
  service files.
- **Medium - `proj` duplicates Niri and Locus wire details in shell.** Service
  names, paths, interfaces, relation names, and object-path parsing are encoded
  in `install/bin/proj:8` through `install/bin/proj:16` and
  `install/bin/proj:125`. The script has a good transport-boundary comment, but
  it should eventually be replaced by a typed CLI/client so shell hooks do not
  own D-Bus protocol shape.
- **Low - project update commands silently no-op outside a project or focused
  workspace.** `cmd_update`, `cmd_set_current`, and `cmd_clear` return success
  on missing project/workspace at `install/bin/proj:311`,
  `install/bin/proj:325`, and `install/bin/proj:338`. That may be right for
  shell hooks, but it is surprising for manual CLI use.

## Refactor Ideas

- Clear or manifest-manage `~/.local/lib/rsynapse/plugins` before installing the
  current bundled plugins.
- Generate service files with a small structured template helper, or validate
  rendered units after install.
- Promote `proj`'s busctl boundary into a small typed Rust CLI once Locus and
  niri-dbus protocols stabilize.
- Consider separate quiet and strict modes for `proj` so shell hooks can no-op
  while manual commands report missing state.

## Open Questions

- Should `local.sh` start/restart the enabled shell services, or only install
  and enable them?
- Should `proj` live in `install/bin` long term, or under a project/Locus client
  package with tests?

## Verification

- `bash -n install/local.sh` passed.
- `bash -n install/bin/proj` passed.
