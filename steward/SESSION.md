# Project-aware desktop sessions

Session capture and restoration belong to Steward. This adds no daemon and no
project CRUD. Projd remains authoritative for projects and their current CWDs;
Locus owns cross-service links; workspace-dbus owns compositor observation and
optional controls. Application-content restoration is follow-up work.

## Commands

The canonical daemon executable is `rsynapse-steward`; the installer keeps the
short `steward` command alias used in the examples below.

With updated Steward and workspace-dbus running:

```sh
steward session list
steward session save work
steward session show work
steward session restore work
steward session restore       # uses last
```

The local installer registers `org.rsynapse.Steward.service` for on-demand D-Bus
activation through `rsynapse-steward.service` (`Type=dbus`). A first CLI/client
request can start Steward when it is not already running. Installing activation
metadata does not restart an existing owner.

CLI output is JSON. `restore` submits an asynchronous D-Bus job, subscribes to
completion signals and prints its report. A partial restore returns a nonzero
exit status with per-workspace/window issues. It keeps successfully matched
windows, so retrying reconciles them instead of relaunching duplicates. Failed
or ambiguous launches remain recorded for the current source epoch rather than
silently spawning another copy on retry.

The additive `org.rsynapse.Steward.Session1` interface is on the existing
`/org/rsynapse/Steward` object:

- `List() -> as`
- `Get(name) -> Session`, `Save(name) -> Session`
- `Restore(name) -> job-id`
- `GetRestore(job-id) -> RestoreReport`
- `RestoreChanged(job-id, report)` signal

Session, workspace, window, recipe and report records are typed structs.
Operations serialize through a session-specific gate; projection/association
reads use connections separate from bounded signal watchers.

## Capture and identity

Structural workspace/window changes and project assignment changes trigger
capture, without polling, sleeps or debounce-based correctness. Transient
focus/title changes do not checkpoint the desktop. Capture subscribes before
reads and verifies source identity, inventory/state and bindings before commit;
incoherent inventories retry on subscribed events with failure deadlines.

- `current`: observations of the connected desktop, excluding automatic empty
  anonymous edge placeholders without project intent.
- `last`: automatic restore checkpoint for the armed source epoch.
- `previous`: preceding automatic checkpoint.
- Any other name: an explicit save, unaffected by automatic capture.

On the first desktop without a saved `last`, automatic capture is armed. A new
compositor session with an existing checkpoint records `current` independently
and protects `last` until an explicit save or successful restore arms that
source epoch. Source loss never saves the disconnected/empty projection.
Use a named save for an exact intentional checkpoint; named saves remain intact
while windows are closed or rearranged later.

Steward assigns random logical workspace/window identities. Public compositor
IDs and object paths are not persisted in descriptions. Live Locus anchors are
session-only and checked against the workspace service's unique owner and
generation. Reusing a native ID in a new session does not transfer its identity.

Persistent Locus relations:

- `org.rsynapse.session.workspace.project`: logical workspace -> stable project ID.
- `org.rsynapse.session.checkpoint.project`: immutable checkpoint-workspace key
  -> stable project ID. A saved description references this key, so later project
  reassignment/unassignment does not change older named saves.

Live, nonpersistent relations:

- `org.rsynapse.workspace.session-workspace`
- `org.rsynapse.window.session-window`
- `org.rsynapse.session.window.launch` (pending launch identity)
- `org.rsynapse.session.capture` (armed source epoch)

Checkpoint links are acknowledged before the description referencing them is
atomically published. A failure before replacement preserves the previous
description. Unreferenced checkpoint links may remain after an interrupted save;
they are not treated as a saved session or live binding.

Descriptions are stored in
`$XDG_STATE_HOME/rsynapse/steward/sessions.json` (default
`~/.local/state/rsynapse/steward/sessions.json`), with version validation,
private file mode, atomic replacement and file/directory synchronization.
`STEWARD_SESSION_STORE_PATH` overrides the path for isolated environments.

## Launch recipes

V1 reopens application windows, not their tabs/documents/process contents.
Recipes have a provider ID and version so future app-content providers can
extend the model. There are no content-capture hooks in this implementation.

Ghostty has a built-in recipe. It opens a fresh instance/window in the project
CWD with a unique Wayland application ID, preserving reliable correlation even
when ordinary Ghostty windows share a process. Current project CWDs are resolved
from projd at restore time. Missing/removed projects are reported, never registered
again from a stale path.

Other applications use an exact-app-ID desktop entry when available. Supported
Exec field codes are expanded into argv without executing through a shell.
Matching requires both a fresh window with the expected app ID and causal
launch-process identity. Apps forwarding launches to an existing single-instance
process may need an explicit configured launcher. Multiple indistinguishable
windows or missing launch information are reported rather than guessed.

Optional overrides: `$XDG_CONFIG_HOME/rsynapse/session-launchers.json`, or
`STEWARD_SESSION_LAUNCHERS_PATH`:

```json
{
  "org.example.App": {
    "argv": ["example-app", "--app-id=org.example.App.rsynapse.r{restore_id}"],
    "match_app_id": "org.example.App.rsynapse.r{restore_id}"
  }
}
```

The application must actually support the specified arguments. `{restore_id}`
is a fresh launch token; `{cwd}` expands to the project directory. Each argv
element remains one argument, including paths with spaces. Rules load when
Steward starts; save again to capture updated recipes.

## Compositor behavior and limits

Umbriel currently supports restoration controls. Niri observation is unchanged;
Niri reports no restore-control capabilities and restoration reports unsupported.

Restore reuses known logical workspace bindings or matching named workspaces,
otherwise selects unassigned empty workspaces. It preserves existing conflicting
project bindings. Preferred available outputs are used; missing outputs fall
back deterministically to a live output, preserving saved relative slot order.
The current output preference is its connector name, not an EDID identity.

Workspace layout mode, window membership and floating state are restored.
Saved scrolling column/row hints preserve launch order; exact tile trees,
dimensions, floating coordinates, scratchpad/pinned placement and focus replay
are not implemented. Umbriel controls can temporarily change focus while moving
windows or selecting layouts.

Umbriel prunes inactive anonymous empty workspaces and has no runtime persistent
workspace-name operation in the targeted revision. Their logical identities and
project associations remain durable; restore reports a deferred empty slot when
it cannot materialize it. Configure a matching named/static workspace to restore
an empty workspace physically. No compositor config is rewritten by restoration.

## Verification without touching the working desktop

Build test binaries, then run:

```sh
python3 steward/tests/session_restore.py \
  --steward "$PWD/steward/target/debug/rsynapse-steward" \
  --producer "$PWD/workspace-dbus/target/debug/rsynapse-workspace-dbus" \
  --artifacts /tmp/rsynapse-session-test
```

The artifact directory must not exist. Requires Umbriel and Python GI/GTK4.
The test creates two distinct private D-Bus sessions with no activation service
directories, headless Umbriel instances, isolated HOME/XDG directories, separate
projd/Locus/Steward state, and GTK probe applications. It invokes binaries
directly, never host systemd, UWSM or installed session helpers.

Coverage includes disk reload across sessions, source loss preserving the last
checkpoint, new live window identities, project bindings/CWDs, layout/floating
state, output fallback, duplicate avoidance, stale generation rejection,
immutable saved bindings after unassignment, and removed-project handling.
