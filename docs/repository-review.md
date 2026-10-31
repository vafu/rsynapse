# Repository ownership and portability review

Reviewed 2026-10-29. This is a snapshot of findings and recommended boundaries;
follow-up execution is tracked through `proj`.

## Cleanup applied

- Removed the retired agent-generated plan/spec directory.
- Removed `install/bin/legacy-proj`, the replaced Bash implementation. The Rust
  CLI and `install/migrate-projd.py` remain the supported paths; Git history
  retains the old implementation.
- Removed `install/zsh/codex-lanes.zsh` and its unconditional installation. Its
  provider choice and private router functions belong in user dotfiles or the
  agent integration's repository.
- Removed the two legacy `.project.json` files. Current project names/icons
  belong in projd; no remaining repository code reads these files.
- Removed generated picker screenshots under `output/playwright/` and the
  redundant `steward/.cargo/.cargo/config.toml` copy.
- Added ignores for local agent permissions, legacy checkout metadata, generated
  output, and Python bytecode. Made installation/build examples independent of
  a particular home directory.

## Highest-priority portability issues

### 1. Resolved: private GitHub API build dependency

The generic source crate previously depended on `github-dbus-api` through an
absolute private checkout path, coupling shell and Steward builds to one
machine. That dependency supplied JSON records/enums, not D-Bus connectivity.

**Updated user direction:** redefining the client schema in Rsynapse is allowed.
`shell/core/shell-source/src/github/schema.rs` now owns the consumed PR,
activity, review, summary and notification wire types. The source module
re-exports these local records, and examples use the same client-owned types.
The existing D-Bus addresses, JSON representation, listener lifecycle and widget
behavior stay intact. The private API crate is no longer a build dependency.

A generic status-provider/plugin protocol is additional extensibility scope,
not required for this cleanup. The service implementation remains private;
Rsynapse owns the client-side schema it needs to consume the existing protocol.

**Remaining boundary:** `shell-source` still exports service-specific `github`
and `proj` helpers and depends on `proj-model`. Domain clients can become
public UI-free integration modules/crates, with generic Observable/D-Bus
plumbing staying in `shell/core/shell-source`.

The `io.github.vfuchedzhy.*` D-Bus names are an existing external protocol
contract, not filesystem configuration. Renaming those requires coordination
with the service rather than a search-and-replace cleanup.

### 2. Resolved: machine-specific build and registry setup

Removed the Cargo failure guard and isolated-cache configuration, forced
cache/target overrides, `/tmp` build-CWD workaround, Nix-store scanning,
pkg-config stub, and launcher `shell.nix` with its personal theme patch.
Build wrappers now honor the caller's normal Cargo environment. Installers use
`cargo install`; built-artifact staging and Python checks use per-project targets
or the explicit `CARGO_TARGET_DIR`. npm installation also honors its normal
registry configuration.

Root `flake.nix`/`flake.lock` provide a pinned Rust/GTK development environment
for x86_64 and aarch64 Linux. The shell rebuild helper installs to the configured
prefix before restarting units, rather than relying on Home Manager wrappers
executing binaries from a particular target directory.

### 3. Resolved: personal inventory in Grafana definitions

Regenerated generic dashboards through the renderer's new offline `--generic`
mode and removed the tracked color registry. Runtime discovery now renders all
dashboards under ignored `observability/grafana/generated/dashboards` and keeps
stable color assignments in XDG user state. Compose uses generic dashboards by
default; `RSYNAPSE_GRAFANA_DASHBOARDS` selects a local rendered directory.
The renderer rejects runtime output into its tracked template directory.
`metrics` volume identities and dashboard UIDs/queries remain intact.

### 4. The installer mixes artifact installation with user policy

**Resolved:** removed global Git-hook installation/activation and the hook
scripts. Projd already watches registered checkouts' Git metadata directly.
Existing host Git configuration was not modified; install documentation explains
how to inspect/remove the old Rsynapse hooks path.

**Remaining:** `install/local.sh` changes the HTTP/HTTPS default handler and
removes the shell/notifications systemd drop-in
directories. It hardcodes `$HOME/.config` rather than consistently honoring
`XDG_CONFIG_HOME`; `session-services.py` already accepts `--config-home`.

**Recommended boundary:** separate installation from an explicit session-policy
setup step. Make URL-handler selection and compositor/terminal
shortcuts independently selectable integrations. Preserve user drop-ins and
pass resolved XDG paths consistently. An offline staging mode should exercise
the same artifact/template path as the real install.

## Further module and ownership opportunities

### Desktop application discovery and launch adapters

`shell/app/src/desktop_icon.rs`,
`shell/launcher/rsynapse-plugin-launcher/src/lib.rs`, and
`steward/src/session/launch.rs` each discover/parse desktop entries;
`install/bin/rsynapse-open-url` has another parser. A small UI-free desktop-entry
utility crate, preferably backed by an existing parser, could share XDG lookup,
desktop-file identity, icon metadata, and correct Exec field handling.
Here “desktop entry” means a standard Linux `.desktop` application file: its
name, icon, launch command, visibility, and application identity.

Ghostty-specific launch behavior is useful integration code, not generic
project-management behavior. Keep `install/bin/ghostty-project-window` and the
Ghostty recipe in a named terminal adapter; let a generic project-terminal
command resolve CWD and dispatch to the chosen adapter. The existing
`STEWARD_SESSION_LAUNCHERS_PATH` configuration is a useful starting point.
Do not replace the explicit launch recipes with a dynamic plugin framework.

### Workday/activity ownership leaks into the project contract and Locus

`projd/model/src/lib.rs` owns `WorkdayTargets` plus session preference and
agent/day relation names. `steward/src/workdays.rs` persists whole day ledgers
and preferences in Locus metadata; `dashboard-api/src/main.rs` reads/writes
those preferences directly through Locus.

`WorkdayTargets` is specifically the user's three numeric daily preferences:
elapsed span hours, unlocked hours, and active hours (defaults 8, 6, and 4).
This finding concerns where that shared record and preference API belong;
it does not call for removing workday tracking or changing those preferences.

Extract the session/activity DTOs and relation vocabulary into a small typed
Steward-owned contract, and expose activity preferences through their owning
service. Move domain ledger persistence to that owner with an explicit data
migration. Locus should retain cross-service links rather than being the
activity/preferences database; projd's API should stay project-management-only.

### Reusable widgets and surface composition

`shell/app/src/widgets/button_group/` and `level_indicator.rs` are candidates
for `shell/widgets/` crates once their interfaces/styles are consumer-supplied,
following the existing `nerd-icon-picker` precedent. Product status thresholds,
GitHub category policy, and selected-project behavior stay in app widgets.

The bar composition in `shell/app/src/widgets/bar/mod.rs` is also much larger
than the widget guide's 300-line target. Split monitor lifecycle, layout, and
surface composition into private modules first; crate-per-widget extraction is
not necessary for every small component.

### Historical notes and test fixtures

`refactor/` is an older review snapshot that still refers to the replaced
`niri-dbus` project. `projd/notes/project-labels.md` includes local absolute
verification paths and a private goal-note location. Keep useful historical
evidence clearly labeled; put future execution logs in `proj` notes and promote
only reusable contracts/how-to material into repository documentation.

`shell/app/src/widgets/bar/github_status/source/test/checks.rs` embeds a
corporate hostname/repository/PR number. Synthetic `example.test` fixtures
would exercise the same formatting behavior without copying personal work
inventory. Generic `/home/user` paths and monitor IDs in tests are ordinary
fixtures, not machine dependencies.

## Suggested order

Next extract service-specific reactive clients from generic source plumbing.
Further installer policy separation, desktop-entry utilities, activity-contract
ownership, and reusable widget extraction can follow independently. The private
GitHub API build dependency, build/registry hacks, global hooks, and Grafana
inventory have been cleaned up.

## Review and verification scope

This pass reviewed tracked layout, manifests, install/build helpers, service
contracts, and representative integration modules. Existing shell/Steward
working-tree changes were present before the review. No live services were
restarted and the larger proposed code extractions have not been applied.

The initial LSP query returned `server_selection_failed` because no Rust route
was configured. `.lspctl.toml` now registers the already-installed rust-analyzer,
links the seven Cargo workspaces, and has user-authorized declaration trust.
Capability initialization and named references for `legacy_endpoint` and
`projd::watch::start` succeeded.

Verification includes shell syntax, build-wrapper environment preservation,
offline install/staging tests, generic and local dashboard rendering tests,
flake derivation evaluation, ignore checks, and `git diff --check`. No Rust
implementation changed in this pass.
