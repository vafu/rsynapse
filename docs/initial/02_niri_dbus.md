# Milestone 02: niri-dbus

## Goal

Build `niri-dbus` as a narrow, read-only session D-Bus projection of niri IPC
state. The service connects to `NIRI_SOCKET`, mirrors compositor state as D-Bus
objects/properties/signals, and contains no shell UI policy.

V1 covers core shell state only:

- outputs;
- workspaces;
- windows;
- focus;
- keyboard layout;
- overview state;
- config-load health;
- connection health;
- a generation counter for reconnect/rebuild cycles.

Command/control methods are deliberately out of scope for v1.

Use `niri-ipc = "=26.4.0"` exactly. The upstream crate follows niri's version
and does not promise Rust API semver stability.

## D-Bus Shape

Well-known name:

```text
org.rsynapse.Niri
```

Root object:

```text
/org/rsynapse/Niri
```

Managed object shape:

```text
/org/rsynapse/Niri
  org.freedesktop.DBus.ObjectManager
  org.rsynapse.Niri1
    Connected: b
    CompositorVersion: s
    Generation: t
    FocusedOutput: ao
    FocusedWorkspace: ao
    FocusedWindow: ao
    KeyboardLayouts: as
    KeyboardLayoutIndex: y
    OverviewOpen: b
    ConfigLoadFailed: b

/org/rsynapse/Niri/Outputs/<escaped-output-name>
  org.rsynapse.Niri1.Output
    Name: s
    Make: s
    Model: s
    Serial: as
    Focused: b
    CurrentWorkspace: ao
    LogicalX: i
    LogicalY: i
    LogicalWidth: u
    LogicalHeight: u
    Scale: d
    VrrSupported: b
    VrrEnabled: b

/org/rsynapse/Niri/Workspaces/workspace_<id>
  org.rsynapse.Niri1.Workspace
    Id: t
    Name: as
    Index: y
    Output: ao
    Active: b
    Focused: b
    Urgent: b
    ActiveWindow: ao

/org/rsynapse/Niri/Windows/window_<id>
  org.rsynapse.Niri1.Window
    Id: t
    Title: as
    AppId: as
    Pid: ai
    Workspace: ao
    Output: ao
    Focused: b
    Floating: b
    Urgent: b
```

Use standard D-Bus update contracts:

- `org.freedesktop.DBus.Properties.PropertiesChanged`
- `org.freedesktop.DBus.ObjectManager.InterfacesAdded`
- `org.freedesktop.DBus.ObjectManager.InterfacesRemoved`

Encoding rules:

- Use `ao` with zero or one object path for optional object references.
- Use zero-or-one arrays for optional primitive values, such as `as` for
  optional strings and `ai` for optional process IDs.
- Encode dynamic path segments with one helper so output names remain valid
  D-Bus object path elements.
- Keep object paths stable for the lifetime of the producing niri object.

## Scope

- Fetch niri version and output details at startup and after reconnect.
- Subscribe to niri IPC events and fold them into an in-memory model for
  workspaces, windows, focus, keyboard layouts, overview state, and config
  health.
- Export dynamic objects with `zbus` and ObjectManager.
- Emit property and object membership signals on changes.
- Track service health with `Connected` and `Generation`.
- Reconnect when niri restarts or the IPC socket changes.

## Non-Scope

- Bar, workspace, window-title, notification, or OSD behavior.
- Sorting, grouping, label, icon, or theme decisions.
- `rsynapse-shell` request commands.
- Removed filesystem-backend implementation changes.
- Command/control methods in v1. Actions such as focusing workspaces/windows
  need a separate milestone with explicit policy and permission boundaries.
- Polling output state. With `niri-ipc` 26.4.0, output details are refreshed at
  startup and reconnect only.

## Implementation Steps

1. Add `zbus`, `zvariant`, `niri-ipc = "=26.4.0"`, async runtime support, and
   tracing.
2. Organize the crate into small modules:
   - `ipc`: async niri socket client, JSON line protocol, and reconnect loop.
   - `state`: pure state model built around `niri_ipc::state::EventStreamState`.
   - `paths`: object path construction and segment escaping.
   - `dbus`: zbus interfaces and dynamic object registration.
   - `service`: session bus ownership, shared state, event application, and
     signal emission.
3. Use two niri IPC connections:
   - one request connection for `Version` and `Outputs`;
   - one event-stream connection for live state.
4. Rely on `EventStreamState` for live workspace, window, keyboard, overview,
   and config state. Niri sends initial state on the event stream, so do not
   issue separate workspace/window snapshot requests for correctness.
5. Keep output details in the internal state from `Request::Outputs`. Refresh
   them at startup and after reconnect only.
6. Diff state changes into deterministic ObjectManager additions/removals and
   `PropertiesChanged` emissions.
7. On disconnect, keep the root object, set `Connected=false`, remove dynamic
   child objects, and reconnect. After reconnect, rebuild child objects, set
   `Connected=true`, and increment `Generation`.
8. Add a binary entrypoint that owns the session bus name and runs until
   interrupted.

## Performance And Scalability

- No polling: all live workspace/window/focus updates come from the niri event
  stream.
- Keep state folding pure and O(changed object count) where possible; avoid
  full D-Bus object rebuilds on every event.
- Emit only changed properties and object membership deltas.
- Keep D-Bus object methods synchronous over already-held state; never perform
  niri IPC inside property getters.
- Use a single service task to serialize niri events into state updates, then
  emit D-Bus signals from the computed diff.
- Keep output refresh limited to startup/reconnect until niri exposes output
  change events.

## Testing And Live Verification

Unit tests:

- D-Bus path segment encoding.
- Optional D-Bus value encoding.
- Event folding for add, update, remove, focus, keyboard layout, overview, and
  config changes.
- State-to-ObjectManager diff generation.
- Optional object reference encoding.
- Disconnect and reconnect stale-object cleanup.

Live checks:

```sh
cargo test
cargo run
busctl --user tree org.rsynapse.Niri
busctl --user introspect org.rsynapse.Niri /org/rsynapse/Niri
busctl --user get-property org.rsynapse.Niri /org/rsynapse/Niri org.rsynapse.Niri1 Connected
busctl --user monitor org.rsynapse.Niri
```

Compare live D-Bus state against `niri msg` output for outputs, workspaces,
windows, focus, keyboard layout, overview, and config load state.

## Risks

- Object paths and interface names become consumer-facing contracts quickly.
- Niri event ordering can briefly produce cross-reference gaps; clients must
  tolerate optional references and the service must not panic on missing target
  objects.
- Reconnect behavior can leave stale objects if removals are not emitted
  cleanly.
- Output details can be stale until reconnect because `niri-ipc` 26.4.0 does
  not include output-change events in the event stream.
- Optional references and primitive values must keep the zero-or-one array
  convention consistent.
- Command methods can accidentally introduce shell policy or permission risk;
  keep v1 read-only.
- Upstream niri IPC can change; pin the version and keep IPC parsing isolated
  from D-Bus export code.

## Done Criteria

- `niri-dbus` owns `org.rsynapse.Niri` on the session bus.
- Root ObjectManager exposes outputs, workspaces, and windows as typed objects.
- Root properties expose focus, keyboard layout, overview, config health,
  connection health, and generation.
- Initial D-Bus state matches current niri state.
- Niri events update D-Bus properties and object membership without polling.
- Disconnect and reconnect transitions do not leave stale managed objects.
- Tests cover path encoding, state folding, and object lifecycle behavior.
- Shell consumers can subscribe through D-Bus without linking to niri IPC.
- No command/control methods are present in v1.
