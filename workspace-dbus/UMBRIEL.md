# Umbriel backend assessment (historical)

Assessed 2026-10-15 against Umbriel `main` at
[`297a787801e3b933cfa871521a4b586a98caef7f`](https://github.com/noctalia-dev/umbriel/tree/297a787801e3b933cfa871521a4b586a98caef7f).
The backend is now implemented under `service/src/backend/umbriel` and verified
against the installed compositor on an isolated headless session (two outputs
and windows, all layouts, floating, scratchpads and output lifecycle). See
[README.md](README.md) for current usage and test commands. The original
source/documentation assessment below is retained as design context;
implementation progress belongs to the project goal tracker.

## Contract-first decision after review

The implementation direction is now [CONTRACT.md](CONTRACT.md):
an independent `workspace-dbus-api` crate, with Niri and Umbriel implementations
behind it. IDs become opaque strings directly; the earlier numeric-alias and
legacy-wire-preservation proposal below is superseded. The baseline includes
current consumer needs, not every Niri property. Project/goal state and
association decisions remain entirely in projd/steward/Locus; the new bridge
has no Locus dependency or relation-cleanup policy. The URL helper is a small,
separate script adaptation. The source/protocol findings below remain useful,
but its original scope and effort estimate describe the broader initial proposal.

## Conclusion and estimated lift

Umbriel has the event-driven IPC needed to support the existing output,
workspace, window, and focus D-Bus models. No upstream compositor change appears
necessary for a useful first backend. Reuse this service's D-Bus projection and
introduce a backend boundary below it, with compositor-neutral internal state.

The socket client is the small part. Identity, downstream joins, state
reconciliation, and session setup are the substantial work. A D-Bus-compatible
object shape alone does not make the whole Rsynapse session compatible.

Engineering estimates for one developer familiar with this repository:

| Milestone | Estimated effort | Completion meaning |
| --- | --- | --- |
| First useful read-only Umbriel trial | 2–3 days | Reactive bar inventory/focus over the existing D-Bus surface, with explicit limitations on identity integrations and unsupported fields |
| Reliable pluggable bridge plus in-repository integrations | 6–10 days total | Both backends, identity handling, shell/steward/Locus joins, reconnect behavior, and focused integration tests |
| Daily-session migration | 8–15 days total | Also port compositor configuration/shortcuts, external agent/app/build hook identity discovery, URL actions, startup, idle/lock and portal checks |

These are planning ranges, not measured implementation times. External hook
implementations and the user's compositor configuration were not inspected;
they account for much of the last range's uncertainty. Perfect parity for every
Niri-specific geometry/diagnostic field could require upstream IPC additions
and is outside these estimates.

## Documentation and source

Official references:

- [Overview and installation](https://docs.noctalia.dev/umbriel/)
- [IPC](https://docs.noctalia.dev/umbriel/ipc/)
- [Workspaces and selectors](https://docs.noctalia.dev/umbriel/workspaces/)
- [Actions](https://docs.noctalia.dev/umbriel/actions/)
- [Configuration](https://docs.noctalia.dev/umbriel/configuration/)
- [Repository README](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/README.md)

Pinned implementation evidence:

- [Subscription protocol, initial snapshots, keyboard/overview payloads, backpressure](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/server/ipc.cpp#L534-L706)
- [Window and workspace payloads](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/server/ipc_commands.cpp#L575-L760)
- [Output payload](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/server/ipc_commands.cpp#L818-L881)
- [Pointer-preferred versus keyboard-focused output](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/server/server_outputs.cpp#L115-L169)
- [Workspace ID allocation](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/workspace/workspace.cpp#L2225-L2239)
- [Session environment publication](https://github.com/noctalia-dev/umbriel/blob/297a787801e3b933cfa871521a4b586a98caef7f/src/server/server.cpp#L211-L233)

## Transport fit

The socket is `UMBRIEL_SOCKET`, falling back to
`$XDG_RUNTIME_DIR/umbriel-$WAYLAND_DISPLAY.sock`. Requests are newline-delimited
JSON. One-shot replies are `{"ok": ...}` or `{"err": "..."}`.

For the current projection, open one persistent socket and send:

```json
{"cmd":"subscribe","events":["outputs","workspaces","windows","overview","keyboard_layout"]}
```

The first successful subscription data is already event envelopes, not Niri's
`Handled` acknowledgment:

```json
{"event":"windows","data":[]}
```

Each family is a complete replacement snapshot, initially and on changes.
Identical consecutive broadcasts are suppressed upstream. There is no need for
polling, shelling out to `umbriel subscribe`, or querying inventories separately
before subscribing. Outputs have their own live subscription, including
hotplug, modes, scale, power, usable area, focus, and active workspace.

The inspected implementation sends initial families in its own fixed order
(overview, optional keyboard layout, windows, workspaces, outputs for the set
above), independently of request order. With no physical keyboard, the keyboard
event is omitted until one arrives: it must not gate readiness. An unknown
subscription family errors and closes the connection. Define a supported
Umbriel baseline rather than blindly subscribing to future optional families.

Families have no shared transaction/revision marker. Retain latest snapshots,
bootstrap the three inventory families before advertising ready state, and
reconcile cross-family references without exporting dangling object paths.
Handle later temporarily unresolved references explicitly; buffering for a
fixed number of milliseconds cannot establish consistency. Removal/association
cleanup must follow authoritative inventory removal, not temporary filtering
of an unresolved relationship. Exact cross-family atomicity would need upstream
protocol support if a future consumer requires it.

Slow subscribers can be disconnected once queued broadcasts exceed 256 KiB in
the inspected server. Drain the socket independently of D-Bus publication and
avoid an unbounded update queue; reconcile superseded geometry efficiently
without silently dropping observed focus/ownership transitions needed by metrics.
Treat EOF/parse errors as loss of observation, clear live objects and focus,
and bootstrap again. Retry backoff is connection recovery, not state polling.

## Model mapping

| Existing D-Bus data | Umbriel source / handling |
| --- | --- |
| Output name/make/model/serial/physical size | Direct fields; normalize empty serial and unknown/nonpositive physical dimensions |
| Output mode/refresh/scale/transform/logical bounds | Direct data with checked conversions to the existing narrower D-Bus types; `refresh_mhz` is already millihertz |
| Output current workspace/focus | `active_workspace` / `focused`; native `focused` is pointer-preferred, not necessarily the output of keyboard focus. Preserve an empty-output fallback |
| VRR supported/enabled | `adaptive_sync` is nullable; present boolean means reportable support/state, null needs an explicit unknown/compatibility policy |
| Workspace ID | Opaque string, currently connector plus group serial, e.g. `DP-1:1`; not the numeric index |
| Workspace name/index | `name` only counts as an explicit name when `named` is true; `index` is already one-based, at most 64 |
| Workspace active/focused/urgent | `active` and `urgent` map directly; native `focused` means active workspace on the pointer-preferred output |
| Workspace active window | `focused_window`, the remembered window for that workspace, or empty |
| Window ID | Opaque ext-foreign-toplevel identifier string |
| Window title/app ID/PID | Direct fields; normalize unknown PID `-1` to absent |
| Window global `Focused` / root `FocusedWindow` | Use window `active`, NOT window `focused`; the latter is remembered workspace-local focus and can be true on several workspaces |
| Window workspace/output | Direct fields; output must not be derived exclusively through workspace, because scratchpad windows can have no workspace |
| Window floating/urgent | Direct fields |
| Window column/row | Zero-based values for scrolling only; `-1` maps to absent for floating/dwindle/master. Verify conventions when normalizing the Niri backend |
| Window dimensions/position | `w`/`h` and `x`/`y` exist, but do not describe the same complete tile/content coordinate model as Niri |
| Overview | `overview.data.open` |
| Keyboard layouts/index | `keyboard_layout.data.names` / `current_index`; handle no-keyboard initialization |
| Focus timestamps | No equivalent in the inspected window payload; return absent rather than inventing history |
| Config-load failure | No equivalent runtime subscription/query found; unknown must be distinguishable from a verified successful reload |
| Compositor version | No runtime IPC version query found; installed `umbriel --version` identifies the binary on PATH, not necessarily the running compositor |
| Custom mode | Current custom modes are synthesized into the mode list, without an explicit custom-mode flag; exact Niri `CustomMode` parity is unavailable |

For Niri-only required scalar properties, document compatibility defaults and
expose capability/availability metadata. Do not make placeholder tile geometry
or `ConfigLoadFailed=false` look like verified observations. Current bar and
steward core flows do not require exact tile geometry, config failure, or focus
timestamps. The URL helper separately relies on focus timestamps for MRU choice.

There is an additional focus policy decision: the IPC output/workspace `focused`
flags use `Server::preferredOutput()`, which selects the output under the cursor
with a fallback. They do not call its separate keyboard-focused-output helper.
When pointer and keyboard focus diverge, blindly combining those workspace flags
with window `active` could attribute work to the wrong project. For the current
keyboard-work metrics, derive workspace/output from the globally active window
where appropriate and use the native preferred output's active workspace when
there is no application focus. Keep native pointer-preferred state separately
available. Layer keyboard focus and scratchpad windows need explicit handling;
the `layers` family can supply layer focus/output if that distinction is needed.

## Recommended backend boundary

```text
Niri socket -> Niri event reducer --------\
                                         -> typed compositor state
Umbriel socket -> family snapshot reducer/       |
                                                v
                                    shared D-Bus projection
                                                |
                                   shell / steward / other clients
```

Use compiled-in backends selected explicitly, e.g. `--backend niri|umbriel`,
with `auto` only when the environment identifies a single intended compositor.
A nested Umbriel session may inherit `NIRI_SOCKET`; presence of both variables
must not silently select the wrong desktop. Socket overrides are useful for
isolated tests. Backend choice should normally be fixed for the process/session.

Suggested modules are `model`, `backend/{niri,umbriel}`, `projection`, and
`identity`. A small typed backend contract can provide connection metadata,
state updates and capabilities; a static enum is sufficient. Dynamic library
plugins are unnecessary for two socket protocols.

Keep `niri_ipc::EventStreamState` inside the Niri backend. Today `src/state.rs`,
`src/dbus.rs`, and `src/service.rs` all depend directly on Niri types, and the
service loop itself matches Niri events. Replacing only `src/ipc.rs` would leave
the wrong abstraction in place. Reuse object registration/removal and
before/after property change detection, adapted to neutral state. Preserve
ObjectManager and PropertiesChanged semantics, including cross-object effects
of output/workspace/window updates.

The current service also clears Locus workspace subjects in
`Service::apply_object_delta`. That behavior needs the new identity mapping:
never clear Niri relations because an Umbriel numeric alias happens to match.

## Identity and compatibility strategy

Preserve `org.rsynapse.Niri`, the existing interfaces, paths, and property
signatures for an initial trial. Changing `Id: u64` to a string in place is a
wire break. Internally retain native IDs as strings, with a collision-free
numeric alias table for the legacy projection and reverse lookup for actions.
Never truncate strings or treat a hash as guaranteed unique identity.

Add identity metadata alongside the compatibility surface: backend, native ID,
live instance/epoch identity, and a typed relation endpoint. Migrate joins to
this metadata rather than path parsing or integer aliases. A future neutral
service name can follow once clients use the neutral contract; it is not a
prerequisite for trying Umbriel.

Legacy numeric aliases are live handles, not persistent keys. Define bridge
restart, socket reconnect, compositor restart, and output-group recreation
behavior explicitly, including retiring aliases and rebuilding relations.
Current workspace IDs are allocated by `WorkspaceGroup`; neither those IDs nor
window identifiers promise persistent identity across compositor restarts.
Reusing old numeric aliases or old `org.rsynapse.niri.*` relation keys across
backends can attach projects/agents to unrelated objects.

Use backend/instance-qualified keys for live associations. Durable named
workspace preferences need backend plus output identity plus explicit name:
Umbriel permits the same name independently on multiple outputs. Keep project
identity in projd. Do not automatically reinterpret existing Niri associations
as Umbriel associations; any desired preference migration needs an explicit
mapping. Numeric aliases alone cannot make external AgentDBus IDs match.

## Downstream work discovered

- `shell/app/src/widgets/bar/niri.rs` hardcodes bus/interfaces/path prefixes and
  exposes numeric IDs. `workspaces.rs` sorts windows by column/row/ID. Dwindle
  and master currently have no column/row ordering, so define a neutral display
  order if layout order is desired. Pinned and scratchpad visibility also need
  a shell decision: filtering solely by home workspace hides some visible
  windows. These are product choices above the bridge.
- `shell/app/src/widgets/bar/window_tile/agent/source/actual.rs` joins directly
  against authoritative AgentDBus window IDs; `steward/src/agent_metrics/source.rs`
  parses AgentDBus `WindowId` strings into numbers. Both need native identity
  translation, and the external producer must learn Umbriel discovery.
- Window app-instance and build-invocation relations use numeric
  `locus::keys::NIRI_WINDOW_ID`. Audit their external producers as well as shell
  readers. PID is useful evidence but not a unique substitute for window ID.
- Steward's naming, project binding, focus metrics and attribution assume
  numeric workspace/window paths and Niri stable-key kinds. Update these joins
  and public binding arguments as necessary. Names are already managed in
  Locus, so lack of an Umbriel runtime workspace-rename action does not block
  the current Rsynapse display naming behavior.
- `locus/src/lib.rs` defines the current Niri key kinds. Locus's store already
  supports arbitrary typed keys; introduce new keys/consumer policy, not a
  mirrored compositor database or a new relation storage engine.
- `install/bin/rsynapse-open-url` directly queries and focuses Niri and chooses
  the MRU browser using Niri timestamps. Port it to the bridge/native backend;
  Umbriel has `window-focus:<native-id>`, but no equivalent focus timestamp
  payload. Bridge-observed MRU would start with unknown history on startup.
- `install/dbus-1/services/org.rsynapse.Niri.service.in` activates the bridge.
  Configure backend selection there or in service configuration. Umbriel
  publishes `UMBRIEL_SOCKET` to systemd and D-Bus activation before starting
  `umbriel-session.target`; verify Rsynapse's currently default-target shell
  units start/restart with the intended environment and stop with the session.
- Steward idle input uses the standard Wayland idle-notify protocol, supported
  by Umbriel. Lock tracking uses logind `LockedHint`; verify the selected locker
  updates it. Full session configuration, portal setup, and shortcuts live
  partly outside this repository. Umbriel can run independently of Noctalia.

## Implementation order and verification evidence needed

1. Extract neutral state and the backend contract while keeping the Niri
   backend's existing behavior. Verify Niri object lifecycle/focus regression.
2. Add typed Umbriel DTOs, async subscription and family reconciliation, plus
   legacy projection and explicit capabilities. Exercise a private D-Bus and
   fake socket with initial family ordering, empty keyboard state, removals,
   malformed messages, EOF, reconnect, and missing referenced objects.
3. Adapt identity joins in shell/steward/Locus and external hooks. Verify equal
   numeric aliases across backends cannot share relations, duplicate workspace
   names on two outputs remain distinct, and bridge/compositor restarts do not
   transfer project/agent ownership to unrelated objects.
4. Run a nested Umbriel trial with an explicitly selected backend on a private
   D-Bus, since both compositors may coexist. Check scrolling/dwindle/master,
   multiple outputs, pointer/keyboard focus divergence, empty-workspace focus,
   layer keyboard focus, scratchpads,
   pins, tab groups, output enable/disable/hotplug and property signals.
5. Validate native session startup/shutdown, idle/lock work attribution, agent
   badges, project/goal context, URL focus, notifications and portals. Retain a
   separately selectable Niri session for comparing behavior.

Upstream already has IPC harness coverage for window/workspace/layer state and
output state in `tests/harness/checks/session/ipc_state.sh` and
`tests/harness/checks/output/ipc_state.sh`. This supports the protocol assessment
but does not replace adapter or live Rsynapse tests. None of those runtime
checks were run for this research-only assessment. Semantic navigation was
unavailable: `lspctl references` returned `server_selection_failed` with reason
`No route matches the target file`; local analysis used source reads/searches.
